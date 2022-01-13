use serde::Deserialize;
use std::fs::File;
use std::path::PathBuf;

use crate::transactions::{Amount, Client, Transaction, TransactionId, TransactionValidationError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TransactionRecordKind {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Debug, Deserialize)]
pub struct TransactionRecord {
    #[serde(rename = "type")]
    kind: TransactionRecordKind,
    client: Client,
    tx: TransactionId,
    amount: Option<Amount>,
}

impl std::convert::TryFrom<TransactionRecord> for Transaction {
    type Error = TransactionValidationError;

    fn try_from(record: TransactionRecord) -> Result<Self, Self::Error> {
        match record.kind {
            TransactionRecordKind::Deposit => {
                if let Some(amount) = record.amount {
                    return Transaction::new_deposit(record.client, record.tx, amount);
                }
                Err(TransactionValidationError::InvalidAmount)
            }
            TransactionRecordKind::Withdrawal => {
                if let Some(amount) = record.amount {
                    return Transaction::new_withdrawal(record.client, record.tx, amount);
                }
                Err(TransactionValidationError::InvalidAmount)
            }
            TransactionRecordKind::Dispute => {
                Ok(Transaction::new_dispute(record.client, record.tx))
            }

            TransactionRecordKind::Resolve => {
                Ok(Transaction::new_resolve(record.client, record.tx))
            }
            TransactionRecordKind::Chargeback => {
                Ok(Transaction::new_chargeback(record.client, record.tx))
            }
        }
    }
}

pub fn parse_from_file(input_path: PathBuf) -> anyhow::Result<Vec<TransactionRecord>> {
    let file = File::open(input_path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(file);

    let mut records = vec![];
    for result in rdr.deserialize() {
        let result: Result<TransactionRecord, _> = result;
        if let Ok(record) = result {
            records.push(record);
        };
    }
    Ok(records)
}
