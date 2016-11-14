extern crate parking_lot;
extern crate ordermap;
extern crate chrono;

use std::usize::MAX as USIZE_MAX;
use std::result::Result;
use std::mem::transmute;
use std::sync::Arc;
use std::time::{Duration, Instant};

use self::parking_lot::{Condvar, Mutex, RwLock};

use self::ordermap::OrderMap;

use self::chrono::Duration as ChDuration;

pub struct Event {
    mutex: Mutex<bool>,
    condvar: Condvar,
    auto_reset: bool,
    map: RwLock<OrderMap<MutexKey, CondvarWithId>>,
}

#[derive(PartialEq, Eq, Hash)]
struct MutexKey {
    mutex: * const Mutex<usize>,
}

unsafe impl Send for MutexKey {}
unsafe impl Sync for MutexKey {}

struct CondvarWithId {
    condvar: * const Condvar,
    id: usize,
    kind: WaitFor,
}

unsafe impl Send for CondvarWithId {}
unsafe impl Sync for CondvarWithId {}

enum WaitFor {
    Any,
    All,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WaitTimeoutResult {
    timed_out: bool,
}

impl WaitTimeoutResult {
    pub fn timed_out(&self) -> bool {
        self.timed_out
    }
}

impl From<parking_lot::WaitTimeoutResult> for WaitTimeoutResult {
    fn from(wtr: parking_lot::WaitTimeoutResult) -> Self {
        WaitTimeoutResult { timed_out: wtr.timed_out() }
    }
}

impl Event {
    pub fn new(initial_signaled: bool, auto_reset: bool) -> Result<Self, ()> {
        Ok(Event {
            mutex: Mutex::new(initial_signaled),
            condvar: Condvar::new(),
            auto_reset: auto_reset,
            map: RwLock::new(OrderMap::new()),
        })
    }

    pub fn wait(&self) {
        let mut guard = self.mutex.lock();
        if !*guard {
            self.condvar.wait(&mut guard);
            assert!(*guard == true);
        };
        if self.auto_reset {
            *guard = false;
        };
    }

    pub fn wait_for(&self, timeout: Duration) -> WaitTimeoutResult {
        if ChDuration::from_std(timeout.clone()).unwrap_or_else(|_e| {
            panic!("Time period too large.");
        }).num_milliseconds() < 0 {
            panic!("Cannot wait for a negative time period.");
        };
        self.wait_until(Instant::now() + timeout)
    }

    pub fn wait_until(&self, timeout: Instant) -> WaitTimeoutResult {
        if timeout < Instant::now() {
            panic!("Cannot wait for a previous time.");
        };
        let mut ret_value = WaitTimeoutResult { timed_out: false };
        let mut guard = self.mutex.lock();
        if !*guard {
            let result = self.condvar.wait_until(&mut guard, timeout);
            ret_value = WaitTimeoutResult::from(result);
            assert!(*guard == true || ret_value.timed_out());
        };
        if self.auto_reset {
            *guard = false;
        };
        ret_value
    }

    pub fn notify(&self) {
        let mut guard = self.mutex.lock();
        *guard = true;
        self.condvar.notify_all();
        let map = self.map.read();
        if map.len() != 0 {
            for (key, value) in map.iter() {
                let mutex = unsafe { key.mutex.as_ref().unwrap() };
                let condvar = unsafe { value.condvar.as_ref().unwrap() };
                let mut guard = mutex.lock();
                match value.kind {
                    WaitFor::Any => *guard = value.id,
                    WaitFor::All => *guard += value.id,
                };
                condvar.notify_all();
            };
        };
    }

    pub fn unnotify(&self) {
        let mut guard = self.mutex.lock();
        *guard = false;
    }
}

pub fn wait_for_any_with(slice: &[Arc<Event>], timeout: Duration) ->
    Result<usize, WaitTimeoutResult>
{
    if ChDuration::from_std(timeout.clone()).unwrap_or_else(|_e| {
        panic!("Time period too large.");
    }).num_milliseconds() < 0 {
        panic!("Cannot wait for a negative time period.");
    };
    wait_for_any_until_impl(slice, true, Instant::now() + timeout)
}

pub fn wait_for_any_until(slice: &[Arc<Event>], timeout: Instant) ->
    Result<usize, WaitTimeoutResult>
{
    if timeout < Instant::now() {
        panic!("Cannot wait for a previous time.");
    };
    wait_for_any_until_impl(slice, true, timeout)
}

pub fn wait_for_any(slice: &[Arc<Event>]) -> usize {
    wait_for_any_until_impl(slice, false, Instant::now()).unwrap()
}

fn wait_for_any_until_impl(
    slice: &[Arc<Event>],
    with_timeout: bool,
    timeout: Instant
) -> Result<usize, WaitTimeoutResult> {
    let mutex = Mutex::new(USIZE_MAX);
    let condvar = Condvar::new();
    let mutex_ptr = &mutex as * const Mutex<usize>;
    let condvar_ptr = &condvar as * const Condvar;
    let key = MutexKey { mutex: mutex_ptr };
    let id;
    let result;
    {
        let mut guard = mutex.lock();
        for (id, event_ref) in slice.iter().enumerate() {
            let guard2 = event_ref.mutex.lock();
            if *guard2 {
                for i in 0..id {
                    let mut map = slice.get(i).unwrap().map.write();
                    map.remove(&key);
                };
                return Ok(id);
            };
            let mut map = event_ref.map.write();
            map.insert(
                MutexKey { mutex: mutex_ptr },
                CondvarWithId {
                    condvar: condvar_ptr,
                    id: id,
                    kind: WaitFor::Any
                }
            );
        };
        result = if with_timeout {
            let mut result = unsafe {
                transmute::<bool, parking_lot::WaitTimeoutResult>(false)
            };
            while *guard == USIZE_MAX && !result.timed_out() {
                result = condvar.wait_until(&mut guard, timeout.clone());
            };
            id = *guard;
            result.timed_out()
        } else {
            while *guard == USIZE_MAX {
                condvar.wait(&mut guard);
            };
            id = *guard;
            false
        };
    };
    for event_ref in slice.iter() {
        let mut map = event_ref.map.write();
        map.remove(&key);
    };
    if result {
        Err(WaitTimeoutResult { timed_out: true })
    } else {
        Ok(id)
    }
}

pub fn wait_for_all_with(slice: &[Arc<Event>], timeout: Duration) ->
    WaitTimeoutResult
{
    if ChDuration::from_std(timeout.clone()).unwrap_or_else(|_e| {
        panic!("Time period too large.");
    }).num_milliseconds() < 0 {
        panic!("Cannot wait for a negative time period.");
    };
    wait_for_all_until_impl(slice, true, Instant::now() + timeout)
}

pub fn wait_for_all_until(slice: &[Arc<Event>], timeout: Instant) ->
    WaitTimeoutResult
{
    if timeout < Instant::now() {
        panic!("Cannot wait for a previous time.");
    };
    wait_for_all_until_impl(slice, true, timeout)
}

pub fn wait_for_all(slice: &[Arc<Event>]) {
    wait_for_all_until_impl(slice, false, Instant::now());
}

fn wait_for_all_until_impl(
    slice: &[Arc<Event>],
    with_timeout: bool,
    timeout: Instant
) -> WaitTimeoutResult {
    let mutex = Mutex::new(0usize);
    let condvar = Condvar::new();
    let mutex_ptr = &mutex as * const Mutex<usize>;
    let condvar_ptr = &condvar as * const Condvar;
    let from_all = (slice.len() * (slice.len() + 1)) / 2;
    let result;
    {
        let mut guard = mutex.lock();
        for (id, event_ref) in slice.iter().enumerate() {
            let guard2 = event_ref.mutex.lock();
            if *guard2 {
                *guard += id + 1;
                continue;
            };
            let mut map = event_ref.map.write();
            map.insert(
                MutexKey { mutex: mutex_ptr },
                CondvarWithId {
                    condvar: condvar_ptr,
                    id: id + 1,
                    kind: WaitFor::All
                }
            );
        };
        result = if with_timeout {
            let mut result = unsafe {
                transmute::<bool, parking_lot::WaitTimeoutResult>(false)
            };
            while *guard != from_all && !result.timed_out() {
                result = condvar.wait_until(&mut guard, timeout.clone());
            };
            result.timed_out()
        } else {
            while *guard != from_all {
                condvar.wait(&mut guard);
            };
            false
        };
    };
    let key = MutexKey { mutex: mutex_ptr };
    for event_ref in slice.iter() {
        let mut map = event_ref.map.write();
        map.remove(&key);
    };
    WaitTimeoutResult { timed_out: result }
}
