use caracat::models::Probe;
use caracat::rate_limiter::RateLimiter;
use caracat::sender::Sender as CaracatSender;
use std::fmt::Display;
use std::fmt::Formatter;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use tracing::{error, info, trace};

use crate::config::CaracatConfig;

#[allow(dead_code)]
pub struct SendLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
    statistics: Arc<Mutex<SendStatistics>>,
}

impl SendLoop {
    pub fn new(rx: Receiver<Vec<Probe>>, feedback: Sender<bool>, config: CaracatConfig) -> Self {
        let mut rate_limiter = RateLimiter::new(
            config.probing_rate,
            config.batch_size,
            config.rate_limiting_method,
        );

        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();

        let statistics = Arc::new(Mutex::new(SendStatistics::default()));
        let statistics_thr = statistics.clone();

        let mut caracat_sender = CaracatSender::new(
            &config.interface,
            config.src_ipv4_addr,
            config.src_ipv6_addr,
            config.instance_id,
            config.dry_run,
        )
        .unwrap();

        let handle = thread::spawn(move || {
            loop {
                let mut statistics = statistics_thr.lock().unwrap();

                let probes = match rx.recv() {
                    Ok(probes) => probes,
                    Err(error) => {
                        error!("{}", error);
                        break;
                    }
                };

                for probe in probes {
                    trace!("{:?}", probe);
                    statistics.read += 1;

                    if let Some(ttl) = config.min_ttl {
                        if probe.ttl < ttl {
                            trace!("{:?} filter=ttl_too_low", probe);
                            statistics.filtered_low_ttl += 1;
                            continue;
                        }
                    }

                    if let Some(ttl) = config.max_ttl {
                        if probe.ttl > ttl {
                            trace!("{:?} filter=ttl_too_high", probe);
                            statistics.filtered_high_ttl += 1;
                            continue;
                        }
                    }

                    for i in 0..config.packets {
                        trace!(
                            "{:?} id={} packet={}",
                            probe,
                            probe.checksum(config.instance_id),
                            i + 1
                        );
                        match caracat_sender.send(&probe) {
                            Ok(_) => statistics.sent += 1,
                            Err(error) => {
                                statistics.failed += 1;
                                error!("{}", error);
                            }
                        }
                        // Rate limit every `batch_size` packets sent.
                        if (statistics.sent + statistics.failed) % config.batch_size == 0 {
                            rate_limiter.wait();
                        }
                    }

                    if let Some(max_probes) = config.max_probes {
                        if statistics.sent >= max_probes {
                            info!("max_probes reached, exiting...");
                            break;
                        }
                    }
                }

                let rate_limiter_statistics = rate_limiter.statistics().lock().unwrap();
                let rate_limiter_statistics_display = RateLimiterStatistics {
                    average_utilization: rate_limiter_statistics.average_utilization(),
                    average_rate: rate_limiter_statistics.average_rate(),
                };

                info!("{}", statistics);
                info!("{}", rate_limiter_statistics_display);

                feedback.send(true).unwrap();

                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping caracat sending loop");
                    break;
                }
            }
        });

        SendLoop {
            handle,
            stopped,
            statistics,
        }
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub struct SendStatistics {
    pub read: u64,
    pub sent: u64,
    pub failed: u64,
    pub filtered_low_ttl: u64,
    pub filtered_high_ttl: u64,
}

impl Display for SendStatistics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "probes_read={} packets_sent={} packets_failed={} filtered_low_ttl={} filtered_high_ttl={}",
               self.read, self.sent, self.failed, self.filtered_low_ttl, self.filtered_high_ttl)
    }
}

pub struct RateLimiterStatistics {
    pub average_utilization: f64,
    pub average_rate: f64,
}

impl Display for RateLimiterStatistics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "average_rate={} average_utilization={}",
            self.average_rate, self.average_utilization
        )
    }
}
