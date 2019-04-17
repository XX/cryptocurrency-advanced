use serde_json::json;
use exonum::{
    api::node::public::explorer::{TransactionQuery, TransactionResponse},
    crypto::{self, Hash, PublicKey, SecretKey},
    messages::{self, RawTransaction, Signed},
};
use exonum_testkit::{ApiKind, TestKit, TestKitApi, TestKitBuilder};

// Import data types used in tests from the crate where the service is defined.
use cryptocurrency_advanced::{
    api::{WalletInfo, WalletQuery},
    transactions::{CreateWallet, Transfer, Approve},
    wallet::Wallet,
    Service,
};

/// Check that the wallet creation transaction works when invoked via API.
#[test]
fn test_create_wallet() {
    let (mut testkit, api) = create_testkit();
    // Create and send a transaction via API
    let (tx, _) = api.create_wallet("Alice");
    testkit.create_block();
    api.assert_tx_status(tx.hash(), &json!({ "type": "success" }));

    // Check that the user indeed is persisted by the service.
    let wallet = api.get_wallet(tx.author()).unwrap();
    assert_eq!(wallet.pub_key, tx.author());
    assert_eq!(wallet.name, "Alice");
    assert_eq!(wallet.balance, 100);
}

/// Check that the transfer transaction works as intended.
#[test]
fn test_transfer() {
    // Create 2 wallets.
    let (mut testkit, api) = create_testkit();
    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    testkit.create_block();
    api.assert_tx_status(tx_alice.hash(), &json!({ "type": "success" }));
    api.assert_tx_status(tx_bob.hash(), &json!({ "type": "success" }));

    // Create approver's keys
    let (approver_pk, approver_sk) = crypto::gen_keypair();

    // Check that the initial Alice's and Bob's balances persisted by the service.
    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);

    // Transfer funds by invoking the corresponding API method.
    let tx = Transfer::sign(
        &tx_alice.author(),
        &tx_bob.author(),
        &approver_pk,
        10, // transferred amount
        0,  // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block();
    api.assert_tx_status(tx.hash(), &json!({ "type": "success" }));

    // After the transfer transaction is included into a block, we may check new wallet
    // balance of sender.
    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 90);
    assert_eq!(wallet.retained_amount, 10);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);

    // Approve the transfer
    let tx = Approve::sign(
        &approver_pk,
        tx.hash(),
        0,  // seed
        &approver_sk,
    );
    api.approve(&tx);
    testkit.create_block();
    api.assert_tx_status(tx.hash(), &json!({ "type": "success" }));

    // After the approve transaction is included into a block, we may check new wallet
    // balance of receiver.
    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 90);
    assert_eq!(wallet.retained_amount, 0);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 110);
    assert_eq!(wallet.retained_amount, 0);
}

/// Check that a transfer from a non-existing wallet fails as expected.
#[test]
fn test_transfer_from_nonexisting_wallet() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    // Do not commit Alice's transaction, so Alice's wallet does not exist
    // when a transfer occurs.
    testkit.create_block_with_tx_hashes(&[tx_bob.hash()]);

    api.assert_no_wallet(tx_alice.author());
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);

    // Create approver's pub key
    let (approver_pk, _) = crypto::gen_keypair();

    let tx = Transfer::sign(
        &tx_alice.author(),
        &tx_bob.author(),
        &approver_pk,
        10, // transfer amount
        0,  // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block_with_tx_hashes(&[tx.hash()]);
    api.assert_tx_status(
        tx.hash(),
        &json!({ "type": "error", "code": 1, "description": "Sender doesn't exist" }),
    );

    // Check that Bob's balance doesn't change.
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
}

/// Check that a transfer to a non-existing wallet fails as expected.
#[test]
fn test_transfer_to_nonexisting_wallet() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    // Do not commit Bob's transaction, so Bob's wallet does not exist
    // when a transfer occurs.
    testkit.create_block_with_tx_hashes(&[tx_alice.hash()]);

    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
    api.assert_no_wallet(tx_bob.author());

    // Create approver's pub key
    let (approver_pk, _) = crypto::gen_keypair();

    let tx = Transfer::sign(
        &tx_alice.author(),
        &tx_bob.author(),
        &approver_pk,
        10, // transfer amount
        0,  // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block_with_tx_hashes(&[tx.hash()]);
    api.assert_tx_status(
        tx.hash(),
        &json!({ "type": "error", "code": 2, "description": "Receiver doesn't exist" }),
    );

    // Check that Alice's balance doesn't change.
    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
}

/// Check that an overcharge does not lead to changes in sender's and receiver's balances.
#[test]
fn test_transfer_overcharge() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    testkit.create_block();

    // Create approver's pub key
    let (approver_pk, _) = crypto::gen_keypair();

    // Transfer funds. The transfer amount (110) is more than Alice has (100).
    let tx = Transfer::sign(
        &tx_alice.author(),
        &tx_bob.author(),
        &approver_pk,
        110, // transfer amount
        0,   // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block();
    api.assert_tx_status(
        tx.hash(),
        &json!({ "type": "error", "code": 3, "description": "Insufficient currency amount" }),
    );

    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
}

/// Check that an approve non-existing transfer fails as expected.
#[test]
fn test_approve_nonexisting_transfer() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    testkit.create_block();
    api.assert_tx_status(tx_alice.hash(), &json!({ "type": "success" }));
    api.assert_tx_status(tx_bob.hash(), &json!({ "type": "success" }));

    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);

    // Create approver's keys
    let (approver_pk, approver_sk) = crypto::gen_keypair();

    let tx = Transfer::sign(
        &tx_alice.author(),
        &tx_bob.author(),
        &approver_pk,
        10, // transfer amount
        0,  // seed
        &key_alice,
    );
    api.transfer(&tx);

    // Do not commit transfer transaction, so it does not exist when an approve occurs.

    let tx = Approve::sign(
        &approver_pk,
        tx.hash(),
        0,  // seed
        &approver_sk,
    );
    api.approve(&tx);
    testkit.create_block_with_tx_hashes(&[tx.hash()]);
    api.assert_tx_status(
        tx.hash(),
        &json!({ "type": "error", "code": 4, "description": "Transfer doesn't exist" }),
    );

    // Check that Bob's balance doesn't change.
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
}

/// Check that an double approve transfer fails as expected.
#[test]
fn test_double_approve_transfer() {
    let (mut testkit, api) = create_testkit();

    let (tx_alice, key_alice) = api.create_wallet("Alice");
    let (tx_bob, _) = api.create_wallet("Bob");
    testkit.create_block();
    api.assert_tx_status(tx_alice.hash(), &json!({ "type": "success" }));
    api.assert_tx_status(tx_bob.hash(), &json!({ "type": "success" }));

    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 100);
    assert_eq!(wallet.retained_amount, 0);

    // Create approver's keys
    let (approver_pk, approver_sk) = crypto::gen_keypair();

    // Transfer funds
    let tx = Transfer::sign(
        &tx_alice.author(),
        &tx_bob.author(),
        &approver_pk,
        10, // transfer amount
        0,  // seed
        &key_alice,
    );
    api.transfer(&tx);
    testkit.create_block();
    api.assert_tx_status(tx.hash(), &json!({ "type": "success" }));

    let transfer_tx_hash = tx.hash();

    // Approve the transfer
    let tx = Approve::sign(
        &approver_pk,
        transfer_tx_hash,
        0,  // seed
        &approver_sk,
    );
    api.approve(&tx);
    testkit.create_block();
    api.assert_tx_status(tx.hash(), &json!({ "type": "success" }));

    // After the approve transaction is included into a block, we may check new wallet
    // balance of receiver.
    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 90);
    assert_eq!(wallet.retained_amount, 0);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 110);
    assert_eq!(wallet.retained_amount, 0);

    // Approve the transfer again
    let tx = Approve::sign(
        &approver_pk,
        transfer_tx_hash,
        1,  // seed
        &approver_sk,
    );
    api.approve(&tx);
    testkit.create_block();
    api.assert_tx_status(
        tx.hash(),
        &json!({ "type": "error", "code": 4, "description": "Transfer doesn't exist" }),
    );

    // Check those balances doesn't change.
    let wallet = api.get_wallet(tx_alice.author()).unwrap();
    assert_eq!(wallet.balance, 90);
    assert_eq!(wallet.retained_amount, 0);
    let wallet = api.get_wallet(tx_bob.author()).unwrap();
    assert_eq!(wallet.balance, 110);
    assert_eq!(wallet.retained_amount, 0);
}

#[test]
fn test_unknown_wallet_request() {
    let (_testkit, api) = create_testkit();

    // Transaction is sent by API, but isn't committed.
    let (tx, _) = api.create_wallet("Alice");

    api.assert_no_wallet(tx.author());
}

/// Wrapper for the cryptocurrency service API allowing to easily use it
/// (compared to `TestKitApi` calls).
struct CryptocurrencyApi {
    pub inner: TestKitApi,
}

impl CryptocurrencyApi {
    /// Generates a wallet creation transaction with a random key pair, sends it over HTTP,
    /// and checks the synchronous result (i.e., the hash of the transaction returned
    /// within the response).
    /// Note that the transaction is not immediately added to the blockchain, but rather is put
    /// to the pool of unconfirmed transactions.
    fn create_wallet(&self, name: &str) -> (Signed<RawTransaction>, SecretKey) {
        let (pubkey, key) = crypto::gen_keypair();
        // Create a pre-signed transaction
        let tx = CreateWallet::sign(name, &pubkey, &key);

        let data = messages::to_hex_string(&tx);
        let tx_info: TransactionResponse = self
            .inner
            .public(ApiKind::Explorer)
            .query(&json!({ "tx_body": data }))
            .post("v1/transactions")
            .unwrap();
        assert_eq!(tx_info.tx_hash, tx.hash());
        (tx, key)
    }

    fn get_wallet(&self, pub_key: PublicKey) -> Option<Wallet> {
        let wallet_info = self
            .inner
            .public(ApiKind::Service("cryptocurrency"))
            .query(&WalletQuery { pub_key })
            .get::<WalletInfo>("v1/wallets/info")
            .unwrap();

        let to_wallet = wallet_info.wallet_proof.to_wallet.check().unwrap();
        let (_, wallet) = to_wallet.all_entries().find(|(&key, _)| key == pub_key)?;
        wallet.cloned()
    }

    /// Sends a transfer transaction over HTTP and checks the synchronous result.
    fn transfer(&self, tx: &Signed<RawTransaction>) {
        let data = messages::to_hex_string(&tx);
        let tx_info: TransactionResponse = self
            .inner
            .public(ApiKind::Explorer)
            .query(&json!({ "tx_body": data }))
            .post("v1/transactions")
            .unwrap();
        assert_eq!(tx_info.tx_hash, tx.hash());
    }

    /// Sends a approve transaction over HTTP and checks the synchronous result.
    fn approve(&self, tx: &Signed<RawTransaction>) {
        let data = messages::to_hex_string(&tx);
        let tx_info: TransactionResponse = self
            .inner
            .public(ApiKind::Explorer)
            .query(&json!({ "tx_body": data }))
            .post("v1/transactions")
            .unwrap();
        assert_eq!(tx_info.tx_hash, tx.hash());
    }

    /// Asserts that a wallet with the specified public key is not known to the blockchain.
    fn assert_no_wallet(&self, pub_key: PublicKey) {
        let wallet_info: WalletInfo = self
            .inner
            .public(ApiKind::Service("cryptocurrency"))
            .query(&WalletQuery { pub_key })
            .get("v1/wallets/info")
            .unwrap();

        let to_wallet = wallet_info.wallet_proof.to_wallet.check().unwrap();
        assert!(to_wallet.missing_keys().any(|&key| key == pub_key))
    }

    /// Asserts that the transaction with the given hash has a specified status.
    fn assert_tx_status(&self, tx_hash: Hash, expected_status: &serde_json::Value) {
        let info: serde_json::Value = self
            .inner
            .public(ApiKind::Explorer)
            .query(&TransactionQuery::new(tx_hash))
            .get("v1/transactions")
            .unwrap();

        if let serde_json::Value::Object(mut info) = info {
            let tx_status = info.remove("status").unwrap();
            assert_eq!(tx_status, *expected_status);
        } else {
            panic!("Invalid transaction info format, object expected");
        }
    }
}

/// Creates a testkit together with the API wrapper defined above.
fn create_testkit() -> (TestKit, CryptocurrencyApi) {
    let testkit = TestKitBuilder::validator().with_service(Service).create();
    let api = CryptocurrencyApi {
        inner: testkit.api(),
    };
    (testkit, api)
}