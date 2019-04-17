use exonum::{
    blockchain::{ExecutionError, ExecutionResult, Transaction, TransactionContext},
    crypto::{Hash, PublicKey, SecretKey},
    messages::{Message, RawTransaction, Signed},
};
use exonum_derive::{ProtobufConvert, TransactionSet};
use failure::Fail;
use crate::{proto, schema::Schema, CRYPTOCURRENCY_SERVICE_ID};

const ERROR_SENDER_SAME_AS_RECEIVER: u8 = 0;
const ERROR_WRONG_SENDER: u8 = 1;
const ERROR_APPROVER_SAME_AS_SENDER: u8 = 2;
const ERROR_APPROVER_SAME_AS_RECEIVER: u8 = 3;
const ERROR_WRONG_APPROVER: u8 = 4;

/// Error codes emitted by wallet transactions during execution.
#[derive(Debug, Fail)]
#[repr(u8)]
pub enum Error {
    /// Wallet already exists.
    ///
    /// Can be emitted by `CreateWallet`.
    #[fail(display = "Wallet already exists")]
    WalletAlreadyExists = 0,

    /// Sender doesn't exist.
    ///
    /// Can be emitted by `Transfer` or `Approve`.
    #[fail(display = "Sender doesn't exist")]
    SenderNotFound = 1,

    /// Receiver doesn't exist.
    ///
    /// Can be emitted by `Transfer`, `Approve` or `Issue`.
    #[fail(display = "Receiver doesn't exist")]
    ReceiverNotFound = 2,

    /// Insufficient currency amount.
    ///
    /// Can be emitted by `Transfer` or `Approve`.
    #[fail(display = "Insufficient currency amount")]
    InsufficientCurrencyAmount = 3,

    /// Transfer doesn't exist.
    ///
    /// Can be emitted by `Approve`.
    #[fail(display = "Transfer doesn't exist")]
    TransferNotFound = 4,
}

impl From<Error> for ExecutionError {
    fn from(value: Error) -> ExecutionError {
        let description = format!("{}", value);
        ExecutionError::with_description(value as u8, description)
    }
}

/// Transfer `amount` of the currency from one wallet to another with approval by a third party.
#[derive(Clone, Copy, Debug, ProtobufConvert)]
#[exonum(pb = "proto::Transfer", serde_pb_convert)]
pub struct Transfer {
    /// `PublicKey` of sender's wallet.
    pub from: PublicKey,
    /// `PublicKey` of receiver's wallet.
    pub to: PublicKey,
    /// `PublicKey` of the transaction approver.
    pub approver: PublicKey,
    /// Amount of currency to transfer.
    pub amount: u64,
    /// Auxiliary number to guarantee [non-idempotence][idempotence] of transactions.
    ///
    /// [idempotence]: https://en.wikipedia.org/wiki/Idempotence
    pub seed: u64,
}

/// Approve the transfer transaction.
#[derive(Clone, Debug, ProtobufConvert)]
#[exonum(pb = "proto::Approve", serde_pb_convert)]
pub struct Approve {
    /// `PublicKey` of the transaction approver.
    pub approver: PublicKey,
    /// `Hash` of the transfer to approve.
    pub transfer_tx_hash: Hash,
    /// Auxiliary number to guarantee [non-idempotence][idempotence] of transactions.
    ///
    /// [idempotence]: https://en.wikipedia.org/wiki/Idempotence
    pub seed: u64,
}

/// Issue `amount` of the currency to the `wallet`.
#[derive(Serialize, Deserialize, Clone, Debug, ProtobufConvert)]
#[exonum(pb = "proto::Issue")]
pub struct Issue {
    /// Issued amount of currency.
    pub amount: u64,
    /// Auxiliary number to guarantee [non-idempotence][idempotence] of transactions.
    ///
    /// [idempotence]: https://en.wikipedia.org/wiki/Idempotence
    pub seed: u64,
}

/// Create wallet with the given `name`.
#[derive(Serialize, Deserialize, Clone, Debug, ProtobufConvert)]
#[exonum(pb = "proto::CreateWallet")]
pub struct CreateWallet {
    /// Name of the new wallet.
    pub name: String,
}

/// Transaction group.
#[derive(Serialize, Deserialize, Clone, Debug, TransactionSet)]
pub enum WalletTransactions {
    /// Transfer tx.
    Transfer(Transfer),
    /// Approve tx.
    Approve(Approve),
    /// Issue tx.
    Issue(Issue),
    /// CreateWallet tx.
    CreateWallet(CreateWallet),
}

impl CreateWallet {
    #[doc(hidden)]
    pub fn sign(name: &str, pk: &PublicKey, sk: &SecretKey) -> Signed<RawTransaction> {
        Message::sign_transaction(
            Self {
                name: name.to_owned(),
            },
            CRYPTOCURRENCY_SERVICE_ID,
            *pk,
            sk,
        )
    }
}

impl Transfer {
    #[doc(hidden)]
    pub fn sign(
        &pk: &PublicKey,
        &to: &PublicKey,
        &approver: &PublicKey,
        amount: u64,
        seed: u64,
        sk: &SecretKey,
    ) -> Signed<RawTransaction> {
        Message::sign_transaction(
            Self { from: pk, to, approver, amount, seed },
            CRYPTOCURRENCY_SERVICE_ID,
            pk,
            sk,
        )
    }
}

impl Transaction for Transfer {
    fn execute(&self, mut context: TransactionContext) -> ExecutionResult {
        let from = &context.author();
        let hash = context.tx_hash();

        let mut schema = Schema::new(context.fork());

        let to = &self.to;
        let approver = &self.approver;
        let amount = self.amount;

        if from != &self.from {
            return Err(ExecutionError::new(ERROR_WRONG_SENDER));
        }

        if from == to {
            return Err(ExecutionError::new(ERROR_SENDER_SAME_AS_RECEIVER));
        }

        if approver == from {
            return Err(ExecutionError::new(ERROR_APPROVER_SAME_AS_SENDER));
        }

        if approver == to {
            return Err(ExecutionError::new(ERROR_APPROVER_SAME_AS_RECEIVER));
        }

        let sender = schema.wallet(from)
            .ok_or(Error::SenderNotFound)?;
        let _receiver = schema.wallet(to)
            .ok_or(Error::ReceiverNotFound)?;

        if sender.balance < amount {
            Err(Error::InsufficientCurrencyAmount)?
        }

        schema.retain_amount_from_wallet_balance(sender, amount, &hash, *self);
        Ok(())
    }
}

impl Approve {
    #[doc(hidden)]
    pub fn sign(
        &pk: &PublicKey,
        transfer_tx_hash: Hash,
        seed: u64,
        sk: &SecretKey,
    ) -> Signed<RawTransaction> {
        Message::sign_transaction(
            Self { approver: pk, transfer_tx_hash, seed },
            CRYPTOCURRENCY_SERVICE_ID,
            pk,
            sk,
        )
    }
}

impl Transaction for Approve {
    fn execute(&self, mut context: TransactionContext) -> ExecutionResult {
        let approver = &context.author();
        let hash = &context.tx_hash();
        let transfer_tx_hash = &self.transfer_tx_hash;

        let mut schema = Schema::new(context.fork());

        let transfer = schema.transfer(transfer_tx_hash)
            .ok_or(Error::TransferNotFound)?;

        let from = &transfer.from;
        let to = &transfer.to;
        let amount = transfer.amount;

        if approver != &transfer.approver {
            return Err(ExecutionError::new(ERROR_WRONG_APPROVER));
        }

        let sender = schema.wallet(from)
            .ok_or(Error::SenderNotFound)?;
        let receiver = schema.wallet(to)
            .ok_or(Error::ReceiverNotFound)?;

        if sender.retained_amount < amount {
            Err(Error::InsufficientCurrencyAmount)?
        }

        schema.decrease_retained_amount(sender, amount, hash, transfer_tx_hash);
        schema.increase_wallet_balance(receiver, amount, hash);

        Ok(())
    }
}

impl Transaction for Issue {
    fn execute(&self, mut context: TransactionContext) -> ExecutionResult {
        let pub_key = &context.author();
        let hash = context.tx_hash();

        let mut schema = Schema::new(context.fork());

        if let Some(wallet) = schema.wallet(pub_key) {
            let amount = self.amount;
            schema.increase_wallet_balance(wallet, amount, &hash);
            Ok(())
        } else {
            Err(Error::ReceiverNotFound)?
        }
    }
}

impl Transaction for CreateWallet {
    fn execute(&self, mut context: TransactionContext) -> ExecutionResult {
        let pub_key = &context.author();
        let hash = context.tx_hash();

        let mut schema = Schema::new(context.fork());

        if schema.wallet(pub_key).is_none() {
            let name = &self.name;
            schema.create_wallet(pub_key, name, &hash);
            Ok(())
        } else {
            Err(Error::WalletAlreadyExists)?
        }
    }
}