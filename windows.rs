extern crate winapi;
extern crate kernel32;
extern crate chrono;

use std::usize;

use std::ptr::{null, null_mut};
use std::result::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};

use self::winapi::winnt::{HANDLE, MAXIMUM_WAIT_OBJECTS};
use self::winapi::winbase::{INFINITE, WAIT_OBJECT_0};
use self::winapi::winerror::WAIT_TIMEOUT;
use self::winapi::minwindef::{BOOL, DWORD, TRUE, FALSE};
use self::kernel32::{CreateEventW, CloseHandle, WaitForSingleObject};
use self::kernel32::{SetEvent, ResetEvent, WaitForMultipleObjects};

use self::chrono::Duration as ChDuration;

pub struct Event {
    handle: HANDLE,
}

unsafe impl Send for Event {}
unsafe impl Sync for Event {}

#[derive(Copy, Clone)]
enum WaitFor {
    Any,
    All,
}

impl Into<BOOL> for WaitFor {
    fn into(self) -> BOOL {
        match self {
            WaitFor::Any => FALSE,
            WaitFor::All => TRUE,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct WaitTimeoutResult {
    timed_out: bool,
}

impl WaitTimeoutResult {
    pub fn timed_out(&self) -> bool {
        self.timed_out
    }
}

impl Event {
    pub fn new(initial_signaled: bool, auto_reset: bool) -> Result<Self, ()> {
        let handle = unsafe {
            CreateEventW(
                null_mut(),
                !auto_reset as BOOL,
                initial_signaled as BOOL,
                null()
            )
        };
        if handle == null_mut() {
            Err(())
        } else {
            Ok(Event{ handle: handle })
        }
    }

    pub fn wait(&self) {
        self.wait_ms(INFINITE);
    }

    pub fn wait_for(&self, timeout: Duration) -> WaitTimeoutResult {
        let ms = ChDuration::from_std(timeout).unwrap_or_else(|_e| {
            panic!("Time period too large.");
        }).num_milliseconds();
        if ms < 0 {
            panic!("Cannot wait for a negative time period.");
        };
        if ms >= INFINITE as i64 {
            panic!("Time period too large.");
        };
        self.wait_ms(ms as DWORD)
    }

    pub fn wait_until(&self, timeout: Instant) -> WaitTimeoutResult {
        let now = Instant::now();
        if timeout < now {
            panic!("Cannot wait for a previous time.");
        };
        self.wait_for(timeout - now)
    }

    fn wait_ms(&self, ms: DWORD) -> WaitTimeoutResult {
        let mut result = INFINITE;
        while result != WAIT_OBJECT_0 && result != WAIT_TIMEOUT {
            result = unsafe { WaitForSingleObject(self.handle, ms) };
        };
        WaitTimeoutResult { timed_out: result == WAIT_TIMEOUT }
    }

    pub fn notify(&self) {
        let result = unsafe { SetEvent(self.handle) };
        assert!(result != 0);
    }

    pub fn unnotify(&self) {
        let result = unsafe { ResetEvent(self.handle) };
        assert!(result != 0);
    }
}

pub fn wait_for_any(slice: &[Arc<Event>]) -> usize {
    wait_for_all_or_any_ms(&slice, WaitFor::Any, INFINITE) as usize
}

pub fn wait_for_all(slice: &[Arc<Event>]) {
    wait_for_all_or_any_ms(&slice, WaitFor::All, INFINITE);
}

pub fn wait_for_any_with(slice: &[Arc<Event>], timeout: Duration) ->
    Result<usize, WaitTimeoutResult>
{
    let result = wait_with(slice, WaitFor::Any, timeout);
    if result == WAIT_TIMEOUT {
        Err(WaitTimeoutResult { timed_out: true })
    } else {
        Ok(result as usize)
    }
}

pub fn wait_for_all_with(slice: &[Arc<Event>], timeout: Duration) ->
    WaitTimeoutResult
{
    let result = wait_with(slice, WaitFor::All, timeout);
    WaitTimeoutResult { timed_out: result == WAIT_TIMEOUT }
}

fn wait_with(slice: &[Arc<Event>], wait_for: WaitFor, timeout: Duration) ->
    DWORD
{
    let ms = ChDuration::from_std(timeout).unwrap_or_else(|_e| {
        panic!("Time period too large.");
    }).num_milliseconds();
    if ms < 0 {
        panic!("Cannot wait for a negative time period.");
    };
    if ms >= INFINITE as i64 {
        panic!("Time period too large.");
    };
    wait_for_all_or_any_ms(slice, wait_for, ms as DWORD)
}

pub fn wait_for_any_until(slice: &[Arc<Event>], timeout: Instant) ->
    Result<usize, WaitTimeoutResult>
{
    let result = wait_until(slice, WaitFor::Any, timeout);
    if result == WAIT_TIMEOUT {
        Err(WaitTimeoutResult { timed_out: true })
    } else {
        Ok(result as usize)
    }
}

pub fn wait_for_all_until(slice: &[Arc<Event>], timeout: Instant) ->
    WaitTimeoutResult
{
    let result = wait_until(slice, WaitFor::All, timeout);
    WaitTimeoutResult { timed_out: result == WAIT_TIMEOUT }
}

fn wait_until(slice: &[Arc<Event>], wait_for: WaitFor, timeout: Instant) ->
    DWORD
{
    let now = Instant::now();
    if timeout < now {
        panic!("Cannot wait for a previous time.");
    };
    wait_with(slice, wait_for, timeout - now)
}

fn wait_for_all_or_any_ms(slice: &[Arc<Event>], wait_for: WaitFor, ms: DWORD) ->
    DWORD
{
    if slice.len() > MAXIMUM_WAIT_OBJECTS as usize {
        panic!("Cannot wait for more than {} events", slice.len())
    };
    let vec_handle = slice.iter()
                            .map(|event_ref| event_ref.handle)
                            .collect::<Vec<_>>();
    let slice_handle = &vec_handle;
    let mut result: DWORD = slice_handle.len() as DWORD;
    let len: DWORD = slice_handle.len() as DWORD;
    while result >= len && result != WAIT_TIMEOUT {
        result = unsafe {
            WaitForMultipleObjects(
                len,
                slice_handle.as_ptr(),
                wait_for.into(),
                ms
            )
        };
    };
    result
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle); };
    }
}
