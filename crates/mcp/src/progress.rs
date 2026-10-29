//! Progress for tool calls that carry a `progressToken`.
//!
//! The engine does not report fine-grained steps, so long calls send an
//! elapsed-time heartbeat: progress is seconds since the call started, without
//! `total`, and every value is strictly greater than the previous one as MCP
//! requires. Clients that extend their request timeout on progress keep long
//! saves (a 383-page PDF takes about a minute) alive.
use std::{future::Future, time::Duration};
use tokio::time::Instant;

pub(crate) const HEARTBEAT: Duration = Duration::from_secs(5);

/// Run `work`, notifying 0 at start, every `interval` while it runs and once
/// when it finishes. Notifications never delay or cancel the work itself.
pub(crate) async fn run<W, N, F>(work: W, interval: Duration, mut notify: N) -> W::Output
where
    W: Future,
    N: FnMut(f64, Option<String>) -> F,
    F: Future<Output = ()>,
{
    let started = Instant::now();
    let mut last = 0.0_f64;
    let next = |last: &mut f64| {
        let value = started.elapsed().as_secs_f64().max(*last + 0.001);
        *last = value;
        value
    };
    notify(0.0, None).await;
    tokio::pin!(work);
    let mut ticker = tokio::time::interval_at(started + interval, interval);
    let output = loop {
        tokio::select! {
            biased;
            output = &mut work => break output,
            _ = ticker.tick() => {
                let value = next(&mut last);
                notify(value, Some(format!("working; {}s elapsed", value.floor()))).await;
            }
        }
    };
    notify(next(&mut last), Some("done".into())).await;
    output
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::run;
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[tokio::test]
    async fn long_work_gets_strictly_increasing_heartbeats() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let output = run(
            async {
                tokio::time::sleep(Duration::from_millis(700)).await;
                7
            },
            Duration::from_millis(150),
            move |value, message| {
                record.lock().unwrap().push((value, message));
                async {}
            },
        )
        .await;
        assert_eq!(output, 7);
        let seen = seen.lock().unwrap();
        // start, about four heartbeats (timer-dependent) and the final value
        assert!(seen.len() >= 4, "{seen:?}");
        assert_eq!(seen[0], (0.0, None));
        assert!(
            seen.windows(2).all(|pair| pair[1].0 > pair[0].0),
            "{seen:?}"
        );
        assert_eq!(seen.last().unwrap().1.as_deref(), Some("done"));
        assert!(seen[1].1.as_deref().unwrap().starts_with("working;"));
    }

    #[tokio::test]
    async fn quick_work_sends_only_start_and_finish() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        run(async {}, Duration::from_secs(5), move |value, _| {
            record.lock().unwrap().push(value);
            async {}
        })
        .await;
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[1] > seen[0]);
    }
}
