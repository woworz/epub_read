//! Local patch: consume due requests even when their windows cannot redraw.
use std::{
    collections::HashMap,
    hash::{BuildHasher, Hash},
    time::Instant,
};

pub(crate) fn drain_due<K: Copy + Eq + Hash, S: BuildHasher>(
    requests: &mut HashMap<K, Instant, S>,
    now: Instant,
    mut redraw_if_visible: impl FnMut(K) -> bool,
) -> (bool, Option<Instant>) {
    let mut repaint_visible = false;
    requests.retain(|id, deadline| {
        if now < *deadline {
            return true;
        }
        repaint_visible |= redraw_if_visible(*id);
        false
    });
    (repaint_visible, requests.values().min().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn empty_queue_does_not_poll() {
        assert_eq!(
            drain_due(
                &mut HashMap::<u32, Instant>::new(),
                Instant::now(),
                |_| panic!()
            ),
            (false, None)
        );
    }

    #[test]
    fn hidden_and_destroyed_requests_are_consumed_without_polling() {
        let now = Instant::now();
        let mut queue = HashMap::from([(1, now), (2, now)]);
        let mut inspected = Vec::new();
        assert_eq!(
            drain_due(&mut queue, now, |id| {
                inspected.push(id);
                false
            }),
            (false, None)
        );
        inspected.sort();
        assert_eq!(inspected, [1, 2]);
        assert!(queue.is_empty());
    }

    #[test]
    fn future_deadline_waits_without_requesting_redraw() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        let mut queue = HashMap::from([(1, later)]);
        assert_eq!(
            drain_due(&mut queue, now, |_| panic!()),
            (false, Some(later))
        );
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn mixed_windows_redraw_visible_and_keep_future_deadline() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        let mut queue = HashMap::from([(1, now), (2, now), (3, later)]);
        assert_eq!(
            drain_due(&mut queue, now, |id| id == 1),
            (true, Some(later))
        );
        assert_eq!(queue, HashMap::from([(3, later)]));
        // Poll from the previous iteration must not survive the next check.
        assert_eq!(
            drain_due(&mut queue, now, |_| panic!()),
            (false, Some(later))
        );
        assert_eq!(drain_due(&mut queue, later, |_| false), (false, None));
    }
}
