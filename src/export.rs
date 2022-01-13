use crate::transactions::Account;
use std::error::Error;
use std::io;

pub fn accounts_info_as_csv<W: io::Write>(
    accounts: Vec<Account>,
    output: W,
) -> Result<(), Box<dyn Error>> {
    let mut wtr = csv::Writer::from_writer(output);
    for account in accounts {
        wtr.serialize(account)?;
    }
    wtr.flush()?;
    Ok(())
}
