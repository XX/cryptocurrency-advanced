#![allow(bare_trait_objects)]
#![allow(renamed_and_removed_lints)]

pub use self::cryptocurrency::{CreateWallet, Issue, Transfer, Wallet};

include!(concat!(env!("OUT_DIR"), "/protobuf_mod.rs"));

use exonum::proto::schema::*;