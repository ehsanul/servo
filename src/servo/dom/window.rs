/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use content::content_task::{ControlMsg, Timer, ExitMsg, global_content, Content};
use dom::bindings::utils::WrapperCache;
use dom::bindings::window;
use dom::event::Event;
use js::jsapi::JSVal;
use util::task::spawn_listener;

use core::comm::{Port, Chan, SharedChan};
use std::timer;
use std::uv_global_loop;

pub enum TimerControlMsg {
    TimerMessage_Fire(~TimerData),
    TimerMessage_Close,
    TimerMessage_TriggerExit //XXXjdm this is just a quick hack to talk to the content task
}

//FIXME If we're going to store the content task, find a way to do so safely. Currently it's
//      only used for querying layout from arbitrary content.
pub struct Window {
    timer_chan: Chan<TimerControlMsg>,
    dom_event_chan: SharedChan<Event>,
    content_task: *mut Content,
    wrapper: WrapperCache
}

impl Drop for Window {
    fn finalize(&self) {
        self.timer_chan.send(TimerMessage_Close);
    }
}

// Holder for the various JS values associated with setTimeout
// (ie. function value to invoke and all arguments to pass
//      to the function when calling it)
pub struct TimerData {
    funval: JSVal,
    args: ~[JSVal],
}

pub fn TimerData(argc: libc::c_uint, argv: *JSVal) -> TimerData {
    unsafe {
        let mut args = ~[];

        let mut i = 2;
        while i < argc as uint {
            args.push(*ptr::offset(argv, i));
            i += 1;
        };

        TimerData {
            funval : *argv,
            args : args,
        }
    }
}

// FIXME: delayed_send shouldn't require Copy
#[allow(non_implicitly_copyable_typarams)]
pub impl Window {
    fn alert(&self, s: &str) {
        // Right now, just print to the console
        io::println(fmt!("ALERT: %s", s));
    }

    fn close(&self) {
        self.timer_chan.send(TimerMessage_TriggerExit);
    }

    fn setTimeout(&self, timeout: int, argc: libc::c_uint, argv: *JSVal) {
        let timeout = int::max(0, timeout) as uint;

        // Post a delayed message to the per-window timer task; it will dispatch it
        // to the relevant content handler that will deal with it.
        timer::delayed_send(&uv_global_loop::get(),
                            timeout,
                            &self.timer_chan,
                            TimerMessage_Fire(~TimerData(argc, argv)));
    }
}

pub fn Window(content_chan: comm::SharedChan<ControlMsg>,
              dom_event_chan: comm::SharedChan<Event>,
              content_task: *mut Content) -> @mut Window {
        
    let win = @mut Window {
        wrapper: WrapperCache::new(),
        dom_event_chan: dom_event_chan,
        timer_chan: do spawn_listener |timer_port: Port<TimerControlMsg>| {
            loop {
                match timer_port.recv() {
                    TimerMessage_Close => break,
                    TimerMessage_Fire(td) => {
                        content_chan.send(Timer(td));
                    }
                    TimerMessage_TriggerExit => content_chan.send(ExitMsg)
                }
            }
        },
        content_task: content_task
    };
    let compartment = global_content().compartment.get();
    window::create(compartment, win);
    win
}
