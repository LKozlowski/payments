use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

mod export;
mod ingest;
mod transactions;

use export::accounts_info_as_csv;
use ingest::parse_from_file;
use transactions::{PaymentEngine, Transaction};

#[derive(Debug, StructOpt)]
#[structopt(name = "payments")]
struct Opt {
    input_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Opt::from_args();
    let mut payment_engine = PaymentEngine::new();
    for record in parse_from_file(opt.input_path)? {
        match Transaction::try_from(record) {
            Ok(transaction) => {
                if let Err(err) = payment_engine.process_transaction(transaction) {
                    log::warn!("unable to process transaction: {}", err);
                }
            }
            Err(err) => {
                log::warn!("unable to parse transaction: {}", err);
            }
        }
    }
    if let Err(err) = accounts_info_as_csv(payment_engine.get_accounts(), io::stdout()) {
        log::warn!("unable to write csv: {}", err);
    }
    Ok(())
}
