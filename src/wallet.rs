use exonum::crypto::{Hash, PublicKey};
use exonum_derive::ProtobufConvert;
use crate::proto;

/// Wallet information stored in the database.
#[derive(Clone, Debug, ProtobufConvert)]
#[exonum(pb = "proto::Wallet", serde_pb_convert)]
pub struct Wallet {
    /// `PublicKey` of the wallet.
    pub pub_key: PublicKey,
    /// Name of the wallet.
    pub name: String,
    /// Current balance of the wallet.
    pub balance: u64,
    /// The amount is retained until the transaction is confirmed.
    pub retained_amount: u64,
    /// Length of the transactions history.
    pub history_len: u64,
    /// `Hash` of the transactions history.
    pub history_hash: Hash,
}

impl Wallet {
    /// Create new Wallet.
    pub fn new(
        pub_key: PublicKey,
        name: &str,
        balance: u64,
        retained_amount: u64,
        history_len: u64,
        history_hash: Hash,
    ) -> Self {
        Self {
            pub_key,
            name: name.to_owned(),
            balance,
            retained_amount,
            history_len,
            history_hash,
        }
    }

    /// Returns a copy of this wallet with updated balance.
    pub fn set_balance(self, balance: u64, history_hash: Hash) -> Self {
        Self::new(
            self.pub_key,
            &self.name,
            balance,
            self.retained_amount,
            self.history_len + 1,
            history_hash,
        )
    }

    /// Returns a copy of this wallet with updated retained amount.
    pub fn set_retained_amount(self, amount: u64, history_hash: Hash) -> Self {
        Self::new(
            self.pub_key,
            &self.name,
            self.balance,
            amount,
            self.history_len + 1,
            history_hash,
        )
    }

    /// Returns a copy of this wallet with updated balance and retained amount.
    pub fn set_balance_and_retained_amount(
        self,
        balance: u64,
        retained_amount: u64,
        history_hash: Hash
    ) -> Self {
        Self::new(
            self.pub_key,
            &self.name,
            balance,
            retained_amount,
            self.history_len + 1,
            history_hash,
        )
    }
}