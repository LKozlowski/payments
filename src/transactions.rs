use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::collections::HashMap;
use thiserror::Error;

pub type Client = u16;
pub type TransactionId = u32;
pub type Amount = Decimal;

#[derive(Error, Debug)]
pub enum TransactionValidationError {
    #[error("amount must be greater that 0.0")]
    InvalidAmount,

    #[error("transaction already processed")]
    Duplicate(TransactionId),

    #[error("insufficient funds")]
    InsufficientFunds,

    #[error("missing funds")]
    MissingAccount,

    #[error("invalid transaction")]
    InvalidTransaction(TransactionId),

    #[error("invalid transaction")]
    DisputeChargeback(TransactionId),

    #[error("frozen account")]
    FrozenAccount,
}

pub enum Transaction {
    Deposit {
        client: Client,
        tx: TransactionId,
        amount: Amount,
        dispute: bool,
        chargeback: bool,
    },
    Withdrawal {
        client: Client,
        tx: TransactionId,
        amount: Amount,
        dispute: bool,
        chargeback: bool,
    },
    Dispute {
        client: Client,
        tx: TransactionId,
    },
    Resolve {
        client: Client,
        tx: TransactionId,
    },
    Chargeback {
        client: Client,
        tx: TransactionId,
    },
}

impl Transaction {
    pub fn new_deposit(
        client: Client,
        tx: TransactionId,
        amount: Amount,
    ) -> Result<Self, TransactionValidationError> {
        if amount <= dec!(0.0) {
            return Err(TransactionValidationError::InvalidAmount);
        };
        let transaction = Self::Deposit {
            client,
            tx,
            amount,
            dispute: false,
            chargeback: false,
        };
        Ok(transaction)
    }

    pub fn new_withdrawal(
        client: Client,
        tx: TransactionId,
        amount: Amount,
    ) -> Result<Self, TransactionValidationError> {
        if amount <= dec!(0.0) {
            return Err(TransactionValidationError::InvalidAmount);
        };

        let transaction = Self::Withdrawal {
            client,
            tx,
            amount,
            dispute: false,
            chargeback: false,
        };
        Ok(transaction)
    }

    pub fn new_dispute(client: Client, tx: TransactionId) -> Self {
        Self::Dispute { client, tx }
    }

    pub fn new_resolve(client: Client, tx: TransactionId) -> Self {
        Self::Resolve { client, tx }
    }
    pub fn new_chargeback(client: Client, tx: TransactionId) -> Self {
        Self::Chargeback { client, tx }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Account {
    client: Client,
    available: Amount,
    held: Amount,
    frozen: bool,
}

impl Account {
    fn new(client: Client) -> Self {
        Self {
            client,
            available: dec!(0.0),
            held: dec!(0.0),
            frozen: false,
        }
    }

    fn total_funds(&self) -> Decimal {
        self.available + self.held
    }
}

impl Serialize for Account {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Account", 4)?;
        state.serialize_field("client", &self.client)?;
        state.serialize_field("available", &self.available.round_dp(4))?;
        state.serialize_field("held", &self.held.round_dp(4))?;
        state.serialize_field("total", &self.total_funds().round_dp(4))?;
        state.serialize_field("locked", &self.frozen)?;
        state.end()
    }
}

pub struct PaymentEngine {
    accounts: HashMap<Client, Account>,
    transactions: HashMap<TransactionId, Transaction>,
}

impl PaymentEngine {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            transactions: HashMap::new(),
        }
    }

    pub fn get_accounts(&self) -> Vec<Account> {
        let mut acc: Vec<Account> = self.accounts.values().cloned().collect();
        acc.sort_by_key(|acc| acc.client);
        acc
    }

    fn process_deposit(&mut self, deposit: Transaction) -> Result<(), TransactionValidationError> {
        if let Transaction::Deposit {
            tx, client, amount, ..
        } = deposit
        {
            if self.transactions.contains_key(&tx) {
                return Err(TransactionValidationError::Duplicate(tx));
            }

            let account = self
                .accounts
                .entry(client)
                .or_insert_with(|| Account::new(client));

            account.available += amount;
            self.transactions.insert(tx, deposit);
        }
        Ok(())
    }

    fn process_withdrawal(
        &mut self,
        withdrawal: Transaction,
    ) -> Result<(), TransactionValidationError> {
        if let Transaction::Withdrawal {
            tx, client, amount, ..
        } = withdrawal
        {
            if self.transactions.contains_key(&tx) {
                return Err(TransactionValidationError::Duplicate(tx));
            }
            let account = match self.accounts.get_mut(&client) {
                Some(account) => account,
                None => {
                    return Err(TransactionValidationError::MissingAccount);
                }
            };
            if account.frozen {
                return Err(TransactionValidationError::FrozenAccount);
            }
            if account.available < amount {
                return Err(TransactionValidationError::InsufficientFunds);
            }
            account.available -= amount;
            self.transactions.insert(tx, withdrawal);
        }

        Ok(())
    }

    fn process_dispute(
        &mut self,
        tx: TransactionId,
        dispute_client: Client,
    ) -> Result<(), TransactionValidationError> {
        match self.transactions.get(&tx) {
            Some(transaction) => match transaction {
                Transaction::Deposit {
                    client,
                    tx,
                    dispute,
                    chargeback,
                    ..
                }
                | Transaction::Withdrawal {
                    client,
                    tx,
                    dispute,
                    chargeback,
                    ..
                } => {
                    if *client != dispute_client {
                        return Err(TransactionValidationError::InvalidTransaction(*tx));
                    };

                    if *chargeback {
                        return Err(TransactionValidationError::DisputeChargeback(*tx));
                    }
                    if *dispute {
                        return Err(TransactionValidationError::Duplicate(*tx));
                    }
                    if !self.accounts.contains_key(client) {
                        return Err(TransactionValidationError::MissingAccount);
                    };
                }
                _ => {}
            },
            None => {
                return Err(TransactionValidationError::InvalidTransaction(tx));
            }
        };

        if let Some(Transaction::Deposit {
            client,
            dispute,
            amount,
            ..
        }) = self.transactions.get_mut(&tx)
        {
            if let Some(account) = self.accounts.get_mut(client) {
                *dispute = true;
                account.available -= *amount;
                account.held += *amount;
            }
        }
        if let Some(Transaction::Withdrawal {
            client,
            dispute,
            amount,
            ..
        }) = self.transactions.get_mut(&tx)
        {
            if let Some(account) = self.accounts.get_mut(client) {
                *dispute = true;
                account.available -= -*amount;
                account.held += -*amount;
            }
        }
        Ok(())
    }

    fn process_resolve(
        &mut self,
        tx: TransactionId,
        resolve_client: Client,
    ) -> Result<(), TransactionValidationError> {
        if !self.transactions.contains_key(&tx) {
            return Err(TransactionValidationError::InvalidTransaction(tx));
        }

        match self.transactions.get_mut(&tx) {
            Some(transaction) => match transaction {
                Transaction::Deposit {
                    client,
                    tx,
                    dispute,
                    chargeback,
                    ..
                }
                | Transaction::Withdrawal {
                    client,
                    tx,
                    dispute,
                    chargeback,
                    ..
                } => {
                    if *client != resolve_client {
                        return Err(TransactionValidationError::InvalidTransaction(*tx));
                    };
                    if !*dispute {
                        return Err(TransactionValidationError::InvalidTransaction(*tx));
                    }
                    if *chargeback {
                        return Err(TransactionValidationError::InvalidTransaction(*tx));
                    }
                }
                _ => {}
            },
            None => return Err(TransactionValidationError::InvalidTransaction(tx)),
        };

        if let Some(Transaction::Deposit {
            client,
            amount,
            dispute,
            ..
        }) = self.transactions.get_mut(&tx)
        {
            if let Some(account) = self.accounts.get_mut(client) {
                account.available += *amount;
                account.held -= *amount;
                *dispute = false;
            } else {
                return Err(TransactionValidationError::MissingAccount);
            }
        }

        if let Some(Transaction::Withdrawal {
            client,
            amount,
            dispute,
            ..
        }) = self.transactions.get_mut(&tx)
        {
            if let Some(account) = self.accounts.get_mut(client) {
                account.available += -*amount;
                account.held -= -*amount;
                *dispute = false;
            } else {
                return Err(TransactionValidationError::MissingAccount);
            }
        }
        Ok(())
    }

    fn process_chargeback(
        &mut self,
        tx: TransactionId,
        chargeback_client: Client,
    ) -> Result<(), TransactionValidationError> {
        if !self.transactions.contains_key(&tx) {
            return Err(TransactionValidationError::InvalidTransaction(tx));
        }

        match self.transactions.get_mut(&tx) {
            Some(transaction) => match transaction {
                Transaction::Deposit {
                    client,
                    tx,
                    dispute,
                    chargeback,
                    ..
                }
                | Transaction::Withdrawal {
                    client,
                    tx,
                    dispute,
                    chargeback,
                    ..
                } => {
                    if *client != chargeback_client {
                        return Err(TransactionValidationError::InvalidTransaction(*tx));
                    };
                    if *chargeback {
                        return Err(TransactionValidationError::Duplicate(*tx));
                    }
                    if !*dispute {
                        return Err(TransactionValidationError::InvalidTransaction(*tx));
                    }
                }
                _ => {}
            },
            None => return Err(TransactionValidationError::InvalidTransaction(tx)),
        };

        if let Some(Transaction::Deposit {
            client,
            amount,
            chargeback,
            ..
        }) = self.transactions.get_mut(&tx)
        {
            if let Some(account) = self.accounts.get_mut(client) {
                account.held -= *amount;
                account.frozen = true;
                *chargeback = true;
            } else {
                return Err(TransactionValidationError::MissingAccount);
            }
        }

        if let Some(Transaction::Withdrawal {
            client,
            amount,
            chargeback,
            ..
        }) = self.transactions.get_mut(&tx)
        {
            if let Some(account) = self.accounts.get_mut(client) {
                account.held -= *amount;
                account.frozen = true;
                *chargeback = true;
            } else {
                return Err(TransactionValidationError::MissingAccount);
            }
        }
        Ok(())
    }

    pub fn process_transaction(
        &mut self,
        transaction: Transaction,
    ) -> Result<(), TransactionValidationError> {
        match transaction {
            Transaction::Deposit { .. } => {
                self.process_deposit(transaction)?;
            }
            Transaction::Withdrawal { .. } => {
                self.process_withdrawal(transaction)?;
            }
            Transaction::Dispute { tx, client, .. } => {
                self.process_dispute(tx, client)?;
            }
            Transaction::Resolve { tx, client, .. } => {
                self.process_resolve(tx, client)?;
            }
            Transaction::Chargeback { tx, client, .. } => {
                self.process_chargeback(tx, client)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_only() {
        let mut engine = PaymentEngine::new();
        engine
            .process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap())
            .unwrap();
        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(100.0));
    }

    #[test]
    fn deposit_duplicate_transactions_are_omitted() {
        let mut engine = PaymentEngine::new();
        engine
            .process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap())
            .unwrap();

        let duplicate_result =
            engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        assert!(duplicate_result.is_err());

        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(100.0));
    }

    #[test]
    fn deposit_only_creates_an_account() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_withdrawal(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        let _ = engine.process_transaction(Transaction::new_resolve(1, 1));
        let _ = engine.process_transaction(Transaction::new_chargeback(1, 1));

        let account = engine.accounts.get(&(1 as Client));
        assert!(account.is_none());

        engine
            .process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap())
            .unwrap();
        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.client, 1);
    }

    #[test]
    fn withdrawal_decreses_available_funds() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_withdrawal(1, 2, dec!(50.0)).unwrap());

        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(50.0));
    }

    #[test]
    fn withdrawal_of_more_funds_than_available_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let result =
            engine.process_transaction(Transaction::new_withdrawal(1, 2, dec!(150.0)).unwrap());

        assert!(result.is_err());
        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(100.0));
    }

    #[test]
    fn dispute_of_non_existing_transaction_returns_error() {
        let mut engine = PaymentEngine::new();
        let result = engine.process_transaction(Transaction::new_dispute(1, 1));
        assert!(result.is_err());
    }

    #[test]
    fn dispute_marks_transaction_as_under_dispute() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());

        engine
            .process_transaction(Transaction::new_dispute(1, 1))
            .unwrap();

        if let Transaction::Deposit { dispute, .. } = engine.transactions.get(&1).unwrap() {
            assert_eq!(dispute, &true);
        } else {
            assert!(false);
        }

        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(0.0));
        assert_eq!(account.held, dec!(100.0));
    }

    #[test]
    fn dispute_duplicate_dispute_does_nothing() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());

        engine
            .process_transaction(Transaction::new_dispute(1, 1))
            .unwrap();

        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(0.0));
        assert_eq!(account.held, dec!(100.0));

        let result = engine.process_transaction(Transaction::new_dispute(1, 1));
        assert!(result.is_err());
        let account = engine.accounts.get(&(1 as Client)).unwrap();
        assert_eq!(account.available, dec!(0.0));
        assert_eq!(account.held, dec!(100.0));
    }

    #[test]
    fn dispute_transaction_that_was_chargebacked_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine
            .process_transaction(Transaction::new_dispute(1, 1))
            .unwrap();
        let _ = engine
            .process_transaction(Transaction::new_chargeback(1, 1))
            .unwrap();
        let result = engine.process_transaction(Transaction::new_dispute(1, 1));
        assert!(result.is_err());
    }

    #[test]
    fn chargeback_of_non_existing_transaction_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let result = engine.process_transaction(Transaction::new_chargeback(1, 2));
        assert!(result.is_err());
    }

    #[test]
    fn chargeback_of_non_disputed_transaction_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let result = engine.process_transaction(Transaction::new_chargeback(1, 1));
        assert!(result.is_err());
    }

    #[test]
    fn chargeback_marks_transaction_as_chargeback() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        let result = engine.process_transaction(Transaction::new_chargeback(1, 1));
        assert!(result.is_ok());

        let tx = engine.transactions.get(&1).unwrap();
        if let Transaction::Deposit { chargeback, .. } = tx {
            assert!(chargeback);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn chargeback_freezes_account() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        let result = engine.process_transaction(Transaction::new_chargeback(1, 1));
        assert!(result.is_ok());
        let account = engine.accounts.get(&1).unwrap();
        assert!(account.frozen);
    }

    #[test]
    fn resolve_of_non_existing_transaction_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let result = engine.process_transaction(Transaction::new_resolve(1, 2));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_of_non_disputed_transaction_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let result = engine.process_transaction(Transaction::new_resolve(1, 1));
        assert!(result.is_err());

        let _ = engine.process_transaction(Transaction::new_withdrawal(1, 1, dec!(100.0)).unwrap());
        let result = engine.process_transaction(Transaction::new_resolve(1, 1));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_of_chargeback_transaction_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        let _ = engine.process_transaction(Transaction::new_chargeback(1, 1));
        let result = engine.process_transaction(Transaction::new_resolve(1, 1));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_clears_dispute() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));

        let tx = engine.transactions.get(&1).unwrap();
        if let Transaction::Deposit { dispute, .. } = tx {
            assert_eq!(*dispute, true);
        } else {
            assert!(false);
        }

        let result = engine.process_transaction(Transaction::new_resolve(1, 1));
        assert!(result.is_ok());

        let tx = engine.transactions.get(&1).unwrap();
        if let Transaction::Deposit { dispute, .. } = tx {
            assert_eq!(*dispute, false);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn dispute_resolve_chargeback_of_mismatched_tx_and_client_returns_error() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());

        let result = engine.process_transaction(Transaction::new_dispute(2, 1));
        assert!(result.is_err());

        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));

        let result = engine.process_transaction(Transaction::new_resolve(2, 1));
        assert!(result.is_err());

        let result = engine.process_transaction(Transaction::new_chargeback(2, 1));
        assert!(result.is_err());
    }

    #[test]
    fn dispute_resolve_of_deposit_with_withdraw() {
        let mut engine = PaymentEngine::new();

        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_withdrawal(1, 2, dec!(50.0)).unwrap());
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(50.0));
            assert_eq!(account.held, dec!(0.0));
        }

        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(-50.0));
            assert_eq!(account.held, dec!(100.0));
        }

        let _ = engine.process_transaction(Transaction::new_resolve(1, 1));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(50.0));
            assert_eq!(account.held, dec!(0.0));
        }
    }

    #[test]
    fn dispute_resolve_of_withdraw() {
        let mut engine = PaymentEngine::new();

        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_withdrawal(1, 2, dec!(50.0)).unwrap());
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(50.0));
            assert_eq!(account.held, dec!(0.0));
        }

        let _ = engine.process_transaction(Transaction::new_dispute(1, 2));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(100.0));
            assert_eq!(account.held, dec!(-50.0));
        }

        let _ = engine.process_transaction(Transaction::new_resolve(1, 2));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(50.0));
            assert_eq!(account.held, dec!(0.0));
        }
    }

    #[test]
    fn chargeback_of_deposit() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_withdrawal(1, 2, dec!(50.0)).unwrap());
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(50.0));
            assert_eq!(account.held, dec!(0.0));
        }

        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(-50.0));
            assert_eq!(account.held, dec!(100.0));
        }

        let _ = engine.process_transaction(Transaction::new_chargeback(1, 1));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(-50.0));
            assert_eq!(account.held, dec!(0.0));
            assert_eq!(account.frozen, true);
        }
    }

    #[test]
    fn frozen_account_only_deposits_works() {
        let mut engine = PaymentEngine::new();
        let _ = engine.process_transaction(Transaction::new_deposit(1, 1, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_deposit(1, 2, dec!(100.0)).unwrap());
        let _ = engine.process_transaction(Transaction::new_dispute(1, 1));
        let _ = engine.process_transaction(Transaction::new_chargeback(1, 1));
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(100.0));
            assert_eq!(account.frozen, true);
        }

        assert!(engine
            .process_transaction(Transaction::new_withdrawal(1, 3, dec!(100.0)).unwrap())
            .is_err());
        assert!(engine
            .process_transaction(Transaction::new_deposit(1, 4, dec!(100.0)).unwrap())
            .is_ok());
        {
            let account = engine.accounts.get(&(1 as Client)).unwrap();
            assert_eq!(account.available, dec!(200.0));
            assert_eq!(account.frozen, true);
        }
    }
}
