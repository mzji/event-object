
#[cfg(not(windows))]
#[path = "generic.rs"]
mod implement;

#[cfg(windows)]
#[path = "windows.rs"]
mod implement;

pub use implement::{Event, WaitTimeoutResult};
pub use implement::{wait_for_any, wait_for_all};
pub use implement::{wait_for_any_with, wait_for_all_with};
pub use implement::{wait_for_any_until, wait_for_all_until};

#[cfg(test)]
mod tests {
    extern crate crossbeam;
    extern crate rand;

    use std::sync::Arc;
    use std::time::Duration;

    use self::rand::{Rng, OsRng};
    use super::{Event, wait_for_any, wait_for_all};
    use super::{wait_for_any_with, wait_for_all_with};

    #[test]
    fn test_wait() {
        let event = Event::new(false, false).unwrap();
        crossbeam::scope(|scope| {
            scope.spawn(|| {
                event.wait();
            });
            event.notify();
        });
    }

    #[test]
    fn test_wait_for() {
        let event = Event::new(false, false).unwrap();
        let result = event.wait_for(Duration::from_millis(200));
        assert!(result.timed_out());
    }

    #[test]
    fn test_wait_for_any() {
        let mut event_vec = vec![];
        for _ in 0..5 {
            event_vec.push(Arc::new(Event::new(false, false).unwrap()));
        };
        crossbeam::scope(|scope| {
            let random_num =
                OsRng::new().unwrap().gen::<usize>() % event_vec.len();
            for (i, event_ref) in event_vec.iter().enumerate() {
                scope.spawn(move || {
                    if i == random_num {
                        event_ref.notify();
                    };
                });
            };
            assert_eq!(random_num, wait_for_any(&event_vec));
        });
    }

    #[test]
    fn test_wait_for_any_with() {
        let mut event_vec = vec![];
        for _ in 0..5 {
            event_vec.push(Arc::new(Event::new(false, false).unwrap()));
        };
        let result = wait_for_any_with(&event_vec, Duration::from_millis(200));
        assert!(result.is_err());
        assert!(result.unwrap_err().timed_out());
    }

    #[test]
    fn test_wait_for_all() {
        let mut event_vec = vec![];
        for _ in 0..5 {
            event_vec.push(Arc::new(Event::new(false, false).unwrap()));
        };
        crossbeam::scope(|scope| {
            for event_ref in event_vec.iter() {
                scope.spawn(move || {
                    event_ref.notify();
                });
            };
            wait_for_all(&event_vec);
        });
    }

    #[test]
    fn test_wait_for_all_with() {
        let mut event_vec = vec![];
        for _ in 0..5 {
            event_vec.push(Arc::new(Event::new(false, false).unwrap()));
        };
        let result = wait_for_all_with(&event_vec, Duration::from_millis(200));
        assert!(result.timed_out());
    }
}
