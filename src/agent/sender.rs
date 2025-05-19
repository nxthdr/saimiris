use caracat::models::Probe;
use caracat::rate_limiter::RateLimiter;
use caracat::sender::Sender as CaracatSender;
use metrics::counter;
use metrics::Label;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use tracing::{error, trace};

use crate::config::AppConfig;

#[allow(dead_code)]
pub struct SendLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
}

impl SendLoop {
    pub fn new(rx: Receiver<Vec<Probe>>, feedback: Sender<bool>, config: AppConfig) -> Self {
        let mut rate_limiter = RateLimiter::new(
            config.caracat.probing_rate,
            config.caracat.batch_size,
            config.caracat.rate_limiting_method,
        );

        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();

        let metrics_labels = vec![Label::new("agent", config.agent.id.to_string())];

        let mut caracat_sender = CaracatSender::new(
            &config.caracat.interface,
            config.caracat.src_ipv4_addr,
            config.caracat.src_ipv6_addr,
            config.caracat.instance_id,
            config.caracat.dry_run,
        )
        .unwrap();

        let handle = thread::spawn(move || {
            loop {
                let probes = match rx.recv() {
                    Ok(probes) => probes,
                    Err(error) => {
                        error!("{}", error);
                        break;
                    }
                };

                counter!("saimiris_sender_read_total", metrics_labels.clone())
                    .increment(probes.len().try_into().unwrap());

                let mut sent = 0;
                let mut failed = 0;
                for probe in probes {
                    trace!("{:?}", probe);

                    if let Some(ttl) = config.caracat.min_ttl {
                        if probe.ttl < ttl {
                            trace!("{:?} filter=ttl_too_low", probe);
                            counter!("saimiris_sender_filtered_total", "agent" => config.agent.id.clone(), "filter" => "ttl_too_low")
                                .increment(1);
                            continue;
                        }
                    }

                    if let Some(ttl) = config.caracat.max_ttl {
                        if probe.ttl > ttl {
                            trace!("{:?} filter=ttl_too_high", probe);
                            counter!("saimiris_sender_filtered_total", "agent" => config.agent.id.clone(), "filter" => "ttl_too_high")
                                .increment(1);
                            continue;
                        }
                    }

                    for i in 0..config.caracat.packets {
                        trace!(
                            "{:?} id={} packet={}",
                            probe,
                            probe.checksum(config.caracat.instance_id),
                            i + 1
                        );
                        match caracat_sender.send(&probe) {
                            Ok(_) => {
                                sent += 1;
                                counter!("saimiris_sender_sent_total", metrics_labels.clone())
                                    .increment(1);
                            }
                            Err(error) => {
                                failed += 1;
                                error!("{}", error);
                                counter!("saimiris_sender_failed_total", metrics_labels.clone())
                                    .increment(1);
                            }
                        }
                        // Rate limit every `batch_size` packets sent.
                        if (sent + failed) % config.caracat.batch_size == 0 {
                            rate_limiter.wait();
                        }
                    }
                }

                feedback.send(true).unwrap();
                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping caracat sending loop");
                    break;
                }
            }
        });

        SendLoop { handle, stopped }
    }
}
