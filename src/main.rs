extern crate chrono;
extern crate ctrlc;
extern crate curl;

use chrono::Local;
use chrono::DateTime;
use curl::easy::Easy;
use std::{
    time::{
        SystemTime,
        UNIX_EPOCH,
        Duration,
    },
    thread,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
use curl::Error;

const URL: &str = "https://www.rust-lang.org/";
const TIME_FORMAT: &str = "%F_%T";

enum InternalMessage {
    StartNewThread,
    DownloadComplete {
        start_time: DateTime<Local>,
        duration: Duration,
    },
    DownloadErr {
        start_time: DateTime<Local>,
        duration: Duration,
        err: Error,
    },
    Terminate,
    SchedulerDone,
}

fn main() {
    println!("Use ctrl+c to quit");

    let running = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::channel();

    let terminate_tx = tx.clone();
    ctrlc::set_handler(move || {
        terminate_tx.send(InternalMessage::Terminate).unwrap();
    }).unwrap();

    let scheduler_running = running.clone();
    let scheduler_tx = tx.clone();
    thread::spawn(move || {
        let mut last_time = SystemTime::now().duration_since(UNIX_EPOCH).expect("Comparing current time to unix epoch");
        while scheduler_running.load(Ordering::SeqCst) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("Comparing current time to unix epoch");
            if last_time.as_secs() / 10 < now.as_secs() / 10 || last_time.as_secs() > now.as_secs() {
                scheduler_tx.send(InternalMessage::StartNewThread).unwrap();
                last_time = now;
            }
            thread::sleep(Duration::from_millis(100));
        }
        scheduler_tx.send(InternalMessage::SchedulerDone).unwrap();
    });

    for signal in &rx {
        match signal {
            InternalMessage::StartNewThread => {
                let download_tx = tx.clone();
                thread::spawn(move || {
                    let mut easy = Easy::new();
                    easy.url(URL).unwrap();
                    easy.write_function(|data| {
                        Ok(data.len())
                    }).unwrap();
                    easy.timeout(Duration::new(8, 0)).unwrap();
                    let start_time = Local::now();
                    match easy.perform() {
                        Ok(_) => {
                            let duration = Local::now().signed_duration_since(start_time).to_std().unwrap();
                            download_tx.send(InternalMessage::DownloadComplete { start_time, duration }).unwrap();
                        }
                        Err(err) => {
                            let duration = Local::now().signed_duration_since(start_time).to_std().unwrap();
                            download_tx.send(InternalMessage::DownloadErr { start_time, duration, err }).unwrap();
                        }
                    }
                });
            }
            InternalMessage::DownloadComplete { start_time, duration } => {
                println!("{}, {}", start_time.format(TIME_FORMAT), duration.as_millis());
            }
            InternalMessage::DownloadErr { start_time, duration, err } => {
                println!("{}, {}, {}", start_time.format(TIME_FORMAT), duration.as_millis(), err);
            }
            InternalMessage::Terminate => {
                println!();
                println!("Shutting down...");
                running.store(false, Ordering::SeqCst);
                break;
            }
            InternalMessage::SchedulerDone => {
                panic!("Scheduler thread ended during runtime");
            }
        }
    }

    // close main thread sender because we no-longer need to make clones of it
    drop(tx);

    // wait for all the other senders to close
    for signal in &rx {
        match signal {
            InternalMessage::SchedulerDone => {
                break;
            }
            _ => {}
        }
    }

    println!("Goodbye.");
}
