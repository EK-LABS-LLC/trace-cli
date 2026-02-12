use clap::Parser




#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {

    #[arg(short, long)]
    name: String
}

fn main() {
    println!("Hello, world!");
}
