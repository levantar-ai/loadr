//! Think time (JMeter-style timers) and constant-throughput pacing.

use std::time::Duration;

use loadr_config::ThinkTimeSpec;
use rand::RngExt;
use rand_distr::{Distribution, Normal};

/// Sample a pause duration from a think-time spec.
pub fn sample_think_time(spec: &ThinkTimeSpec, rng: &mut impl RngExt) -> Duration {
    match spec {
        ThinkTimeSpec::Constant { duration } => duration.as_duration(),
        ThinkTimeSpec::Uniform { min, max } => {
            let lo = min.as_duration().as_secs_f64();
            let hi = max.as_duration().as_secs_f64();
            if hi <= lo {
                return min.as_duration();
            }
            Duration::from_secs_f64(rng.random_range(lo..=hi))
        }
        ThinkTimeSpec::Gaussian { mean, std_dev } => {
            let mu = mean.as_duration().as_secs_f64();
            let sigma = std_dev.as_duration().as_secs_f64();
            let sampled = Normal::new(mu, sigma).map(|n| n.sample(rng)).unwrap_or(mu);
            Duration::from_secs_f64(sampled.max(0.0))
        }
    }
}

/// Constant-throughput pacing: spaces iteration starts so the scenario
/// approaches the target rate (the JMeter constant-throughput timer).
#[derive(Debug)]
pub struct Pacer {
    /// Seconds between iteration starts across the whole scenario.
    interval: f64,
    started: std::time::Instant,
    iterations: u64,
}

impl Pacer {
    /// `iterations_per_second` is scenario-wide; the executor divides among VUs.
    pub fn new(iterations_per_second: f64) -> Self {
        Pacer {
            interval: 1.0 / iterations_per_second.max(1e-9),
            started: std::time::Instant::now(),
            iterations: 0,
        }
    }

    /// Time to wait before starting the next iteration.
    pub fn next_delay(&mut self) -> Duration {
        let due = self.interval * self.iterations as f64;
        self.iterations += 1;
        let elapsed = self.started.elapsed().as_secs_f64();
        if due > elapsed {
            Duration::from_secs_f64(due - elapsed)
        } else {
            Duration::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_config::Dur;
    use rand::SeedableRng;

    fn rng() -> rand::rngs::SmallRng {
        rand::rngs::SmallRng::seed_from_u64(7)
    }

    #[test]
    fn constant_is_exact() {
        let spec = ThinkTimeSpec::Constant {
            duration: Dur::from_millis(250),
        };
        assert_eq!(
            sample_think_time(&spec, &mut rng()),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn uniform_stays_in_range() {
        let spec = ThinkTimeSpec::Uniform {
            min: Dur::from_millis(100),
            max: Dur::from_millis(300),
        };
        let mut r = rng();
        for _ in 0..1000 {
            let d = sample_think_time(&spec, &mut r);
            assert!(d >= Duration::from_millis(100) && d <= Duration::from_millis(300));
        }
    }

    #[test]
    fn gaussian_truncates_at_zero() {
        let spec = ThinkTimeSpec::Gaussian {
            mean: Dur::from_millis(10),
            std_dev: Dur::from_millis(100),
        };
        let mut r = rng();
        for _ in 0..1000 {
            // Must never be negative (would panic in from_secs_f64).
            let _ = sample_think_time(&spec, &mut r);
        }
    }

    #[test]
    fn gaussian_centers_on_mean() {
        let spec = ThinkTimeSpec::Gaussian {
            mean: Dur::from_millis(200),
            std_dev: Dur::from_millis(20),
        };
        let mut r = rng();
        let n = 2000;
        let total: f64 = (0..n)
            .map(|_| sample_think_time(&spec, &mut r).as_secs_f64())
            .sum();
        let avg = total / n as f64;
        assert!((avg - 0.2).abs() < 0.01, "avg={avg}");
    }

    #[test]
    fn pacer_spaces_iterations() {
        let mut p = Pacer::new(100.0); // 10ms interval
        let d0 = p.next_delay();
        assert_eq!(d0, Duration::ZERO, "first iteration starts immediately");
        let d1 = p.next_delay();
        assert!(d1 > Duration::ZERO && d1 <= Duration::from_millis(10));
    }
}
