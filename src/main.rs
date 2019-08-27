extern crate curl;

use std::io::{stdout, Write};
use curl::easy::Easy;
use std::time::Duration;

const URL: &str = "https://www.rust-lang.org/";

fn main() {
    let mut easy = Easy::new();
    easy.url(URL).unwrap();
    easy.write_function(|data| {
        Ok(stdout().write(data).unwrap())
    }).unwrap();
    easy.timeout(Duration::new(8, 0)).unwrap();
    easy.perform().unwrap();
}
