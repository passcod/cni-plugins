use cni::Cni;
use std::process::exit;

mod cni;

fn main() {
    let cni = Cni::load();
    eprintln!("{:?}", cni);
    exit(100);
}
