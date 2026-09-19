use std::{
    ffi::c_void,
    ptr,
    sync::{Arc, Mutex},
    thread,
};

use super::{InputError, SoundEvent, SoundEventSink};

type CGEventTapProxy = *mut c_void;
type CGEventType = u32;
type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CGEventTapCallBack =
    unsafe extern "C" fn(CGEventTapProxy, CGEventType, CGEventRef, *mut c_void) -> CGEventRef;

const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CFStringRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(run_loop: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: CFRunLoopRef);
    fn CFRelease(value: *const c_void);
}

#[derive(Debug)]
pub(crate) struct InputListener {
    run_loop: Arc<Mutex<Option<usize>>>,
    join: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Drop for InputListener {
    fn drop(&mut self) {
        if let Some(run_loop) = *self.run_loop.lock().expect("input run loop mutex poisoned") {
            unsafe {
                CFRunLoopStop(run_loop as CFRunLoopRef);
            }
        }
        if let Some(join) = self
            .join
            .lock()
            .expect("input thread mutex poisoned")
            .take()
        {
            let _ = join.join();
        }
    }
}

pub(crate) fn start_listener(sink: SoundEventSink) -> Result<InputListener, InputError> {
    let trusted = unsafe { CGPreflightListenEventAccess() || CGRequestListenEventAccess() };
    if !trusted {
        return Err(InputError::PermissionDenied);
    }
    let run_loop = Arc::new(Mutex::new(None));
    let thread_run_loop = run_loop.clone();
    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    let sink_pointer = Box::into_raw(Box::new(sink)) as usize;

    let join = thread::Builder::new()
        .name("keyforge-input-macos".into())
        .spawn(move || {
            let sink_pointer = sink_pointer as *mut c_void;
            let tap = unsafe {
                CGEventTapCreate(
                    K_CG_SESSION_EVENT_TAP,
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    1_u64 << K_CG_EVENT_KEY_DOWN,
                    event_tap_callback,
                    sink_pointer,
                )
            };
            if tap.is_null() {
                unsafe {
                    drop(Box::from_raw(sink_pointer as *mut SoundEventSink));
                }
                let _ = ready_sender.send(Err(InputError::PermissionDenied));
                return;
            }

            let source = unsafe { CFMachPortCreateRunLoopSource(ptr::null(), tap, 0) };
            if source.is_null() {
                unsafe {
                    CFRelease(tap);
                    drop(Box::from_raw(sink_pointer as *mut SoundEventSink));
                }
                let _ = ready_sender.send(Err(InputError::Unavailable));
                return;
            }

            let current = unsafe { CFRunLoopGetCurrent() };
            {
                let mut run_loop = thread_run_loop
                    .lock()
                    .expect("input run loop mutex poisoned");
                *run_loop = Some(current as usize);
            }
            unsafe {
                CFRunLoopAddSource(current, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
            }
            let _ = ready_sender.send(Ok(()));
            unsafe {
                CFRunLoopRun();
                CFRelease(source);
                CFRelease(tap);
                drop(Box::from_raw(sink_pointer as *mut SoundEventSink));
            }
        })
        .map_err(|_| {
            unsafe {
                drop(Box::from_raw(sink_pointer as *mut SoundEventSink));
            }
            InputError::Unavailable
        })?;

    match ready_receiver.recv() {
        Ok(Ok(())) => Ok(InputListener {
            run_loop,
            join: Mutex::new(Some(join)),
        }),
        Ok(Err(error)) => {
            let _ = join.join();
            Err(error)
        }
        Err(_) => {
            let _ = join.join();
            Err(InputError::Unavailable)
        }
    }
}

unsafe extern "C" fn event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    if event_type == K_CG_EVENT_KEY_DOWN && !user_info.is_null() {
        let key_code = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE);
        if let Some(sound_event) = classify_macos_key_code(key_code as u16) {
            let sink = &*(user_info as *const SoundEventSink);
            sink(sound_event);
        }
    }
    event
}

fn classify_macos_key_code(key_code: u16) -> Option<SoundEvent> {
    match key_code {
        49 => Some(SoundEvent::Space),
        36 | 76 => Some(SoundEvent::Enter),
        51 | 117 => Some(SoundEvent::Backspace),
        54..=62 => Some(SoundEvent::Modifier),
        53 | 96..=103 | 105 | 107 | 109 | 111 | 113..=126 => None,
        _ => Some(SoundEvent::Normal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_raw_macos_codes_inside_adapter_only() {
        assert_eq!(K_CG_EVENT_TAP_OPTION_LISTEN_ONLY, 1);
        assert_eq!(classify_macos_key_code(49), Some(SoundEvent::Space));
        assert_eq!(classify_macos_key_code(36), Some(SoundEvent::Enter));
        assert_eq!(classify_macos_key_code(51), Some(SoundEvent::Backspace));
        assert_eq!(classify_macos_key_code(56), Some(SoundEvent::Modifier));
        assert_eq!(classify_macos_key_code(0), Some(SoundEvent::Normal));
        assert_eq!(classify_macos_key_code(53), None);
    }
}
