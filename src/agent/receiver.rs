use caracat::models::Reply;
use caracat::receiver::Receiver;
use metrics::counter;
use metrics::Label;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use tracing::{error, trace};

use crate::config::AppConfig;

#[allow(dead_code)]
pub struct ReceiveLoop {
    handle: JoinHandle<()>,
    stopped: Arc<Mutex<bool>>,
}

impl ReceiveLoop {
    pub fn new(tx: Sender<Reply>, config: AppConfig) -> Self {
        // By default if a thread panic, the other threads are not affected and the error
        // is only surfaced when joining the thread. However since this is a long-lived thread,
        // we're not calling join until the end of the process. Since this loop is critical to
        // the process, we don't want it to crash silently. We currently rely on
        // `utilities::exit_process_on_panic` but we might find a better way in the future.
        let stopped = Arc::new(Mutex::new(false));
        let stopped_thr = stopped.clone();

        let tx_thr = tx.clone();
        let metrics_labels = vec![Label::new("agent", config.agent.id.to_string())];

        let handle = thread::spawn(move || {
            let mut receiver = Receiver::new_batch(&config.caracat.interface).unwrap();

            loop {
                let result = receiver.next_reply();
                match result {
                    Ok(reply) => {
                        counter!("saimiris_receiver_received_total", metrics_labels.clone())
                            .increment(1);
                        if !config.caracat.integrity_check
                            || (config.caracat.integrity_check
                                && reply.is_valid(config.caracat.instance_id))
                        {
                            tx_thr.send(reply).unwrap();
                            // TODO: Write round column.
                            // TODO: Compare output with caracal (capture timestamp resolution?)
                        } else {
                            counter!(
                                "saimiris_receiver_received_invalid_total",
                                metrics_labels.clone()
                            )
                            .increment(1);
                        }
                    }
                    Err(error) => {
                        counter!(
                            "saimiris_receiver_received_error_total",
                            metrics_labels.clone()
                        )
                        .increment(1);
                        // TODO: Cleanup this by returning a proper error type,
                        // e.g. ReceiverError::CaptureError(...)
                        match error.downcast_ref::<pcap::Error>() {
                            Some(error) => match error {
                                pcap::Error::TimeoutExpired => {}
                                _ => error!("{:?}", error),
                            },
                            None => {
                                error!("{:?}", error);
                            }
                        }
                    }
                }

                if *stopped_thr.lock().unwrap() {
                    trace!("Stopping receive loop");
                    break;
                }
            }
        });

        ReceiveLoop { handle, stopped }
    }

    #[allow(dead_code)]
    pub fn stop(self) {
        *self.stopped.lock().unwrap() = true;
        self.handle.join().unwrap();
    }
}
