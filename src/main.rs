extern crate chrono;
extern crate ctrlc;
extern crate curl;
extern crate dirs;

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
    fs::File,
    io::{
        BufWriter,
        Write,
    },
};
use curl::Error;

const URL: &str = "https://www.rust-lang.org/";
const FILENAME_TIME_FORMAT: &str = "%F_%H-%M-%S";
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
    let filename = format!("network-log-{}.csv", Local::now().format(FILENAME_TIME_FORMAT));
    let file_path = dirs::home_dir().unwrap().join(".network-downage").join(filename);
    let file_path_display = file_path.display();
    let file_handle = File::create(&file_path).expect("Unable to create log file");
    let mut file_buf = BufWriter::new(file_handle);

    println!("Writing to: {}", file_path_display);

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
                writeln!(&mut file_buf, "{}, {}", start_time.format(TIME_FORMAT), duration.as_millis()).unwrap_or_else(|_err| println!("Unable to write to log"));
                file_buf.flush().unwrap_or_else(|_err| println!("Unable to flush to log"));
            }
            InternalMessage::DownloadErr { start_time, duration, err } => {
                println!("{}, {}, {}", start_time.format(TIME_FORMAT), duration.as_millis(), err);
                writeln!(&mut file_buf, "{}, {}, {}", start_time.format(TIME_FORMAT), duration.as_millis(), err).unwrap_or_else(|_err| println!("Unable to write to log"));
                file_buf.flush().unwrap_or_else(|_err| println!("Unable to flush to log"));
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
