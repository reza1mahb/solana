//! The `transaction` module provides functionality for creating log transactions.

use bincode::serialize;
use chrono::prelude::*;
use hash::Hash;
use plan::{Condition, Payment, Plan};
use rayon::prelude::*;
use signature::{KeyPair, KeyPairUtil, PublicKey, Signature, SignatureUtil};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Transaction {
    pub from: PublicKey,
    pub plan: Plan,
    pub tokens: i64,
    pub last_id: Hash,
    pub sig: Signature,
}

impl Transaction {
    /// Create and sign a new Transaction. Used for unit-testing.
    pub fn new(from_keypair: &KeyPair, to: PublicKey, tokens: i64, last_id: Hash) -> Self {
        let from = from_keypair.pubkey();
        let plan = Plan::Pay(Payment { tokens, to });
        let mut tr = Transaction {
            from,
            plan,
            tokens,
            last_id,
            sig: Signature::default(),
        };
        tr.sign(from_keypair);
        tr
    }

    /// Create and sign a postdated Transaction. Used for unit-testing.
    pub fn new_on_date(
        from_keypair: &KeyPair,
        to: PublicKey,
        dt: DateTime<Utc>,
        tokens: i64,
        last_id: Hash,
    ) -> Self {
        let from = from_keypair.pubkey();
        let plan = Plan::Race(
            (Condition::Timestamp(dt), Payment { tokens, to }),
            (Condition::Signature(from), Payment { tokens, to: from }),
        );
        let mut tr = Transaction {
            from,
            plan,
            tokens,
            last_id,
            sig: Signature::default(),
        };
        tr.sign(from_keypair);
        tr
    }

    fn get_sign_data(&self) -> Vec<u8> {
        serialize(&(&self.plan, &self.tokens, &self.last_id)).unwrap()
    }

    /// Sign this transaction.
    pub fn sign(&mut self, keypair: &KeyPair) {
        let sign_data = self.get_sign_data();
        self.sig = Signature::clone_from_slice(keypair.sign(&sign_data).as_ref());
    }

    /// Verify this transaction's signature and its spending plan.
    pub fn verify(&self) -> bool {
        self.sig.verify(&self.from, &self.get_sign_data()) && self.plan.verify(self.tokens)
    }
}

/// Verify a batch of signatures.
pub fn verify_signatures(transactions: &[Transaction]) -> bool {
    transactions.par_iter().all(|tr| tr.verify())
}

/// Verify a batch of spending plans.
pub fn verify_plans(transactions: &[Transaction]) -> bool {
    transactions.par_iter().all(|tr| tr.plan.verify(tr.tokens))
}

/// Verify a batch of transactions.
pub fn verify_transactions(transactions: &[Transaction]) -> bool {
    verify_signatures(transactions) && verify_plans(transactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::{deserialize, serialize};

    #[test]
    fn test_claim() {
        let keypair = KeyPair::new();
        let zero = Hash::default();
        let tr0 = Transaction::new(&keypair, keypair.pubkey(), 42, zero);
        assert!(tr0.verify());
    }

    #[test]
    fn test_transfer() {
        let zero = Hash::default();
        let keypair0 = KeyPair::new();
        let keypair1 = KeyPair::new();
        let pubkey1 = keypair1.pubkey();
        let tr0 = Transaction::new(&keypair0, pubkey1, 42, zero);
        assert!(tr0.verify());
    }

    #[test]
    fn test_serialize_claim() {
        let plan = Plan::Pay(Payment {
            tokens: 0,
            to: Default::default(),
        });
        let claim0 = Transaction {
            from: Default::default(),
            plan,
            tokens: 0,
            last_id: Default::default(),
            sig: Default::default(),
        };
        let buf = serialize(&claim0).unwrap();
        let claim1: Transaction = deserialize(&buf).unwrap();
        assert_eq!(claim1, claim0);
    }

    #[test]
    fn test_bad_event_signature() {
        let zero = Hash::default();
        let keypair = KeyPair::new();
        let pubkey = keypair.pubkey();
        let mut tr = Transaction::new(&keypair, pubkey, 42, zero);
        tr.sign(&keypair);
        tr.tokens = 1_000_000; // <-- attack!
        assert!(!tr.verify());
    }

    #[test]
    fn test_hijack_attack() {
        let keypair0 = KeyPair::new();
        let keypair1 = KeyPair::new();
        let thief_keypair = KeyPair::new();
        let pubkey1 = keypair1.pubkey();
        let zero = Hash::default();
        let mut tr = Transaction::new(&keypair0, pubkey1, 42, zero);
        tr.sign(&keypair0);
        if let Plan::Pay(ref mut payment) = tr.plan {
            payment.to = thief_keypair.pubkey(); // <-- attack!
        };
        assert!(!tr.verify());
    }

    #[test]
    fn test_overspend_attack() {
        let keypair0 = KeyPair::new();
        let keypair1 = KeyPair::new();
        let zero = Hash::default();
        let mut tr = Transaction::new(&keypair0, keypair1.pubkey(), 1, zero);
        if let Plan::Pay(ref mut payment) = tr.plan {
            payment.tokens = 2; // <-- attack!
        }
        assert!(!tr.verify());

        // Also, ensure all branchs of the plan spend all tokens
        if let Plan::Pay(ref mut payment) = tr.plan {
            payment.tokens = 0; // <-- whoops!
        }
        assert!(!tr.verify());
    }

    #[test]
    fn test_verify_transactions() {
        let alice_keypair = KeyPair::new();
        let bob_pubkey = KeyPair::new().pubkey();
        let carol_pubkey = KeyPair::new().pubkey();
        let last_id = Hash::default();
        let tr0 = Transaction::new(&alice_keypair, bob_pubkey, 1, last_id);
        let tr1 = Transaction::new(&alice_keypair, carol_pubkey, 1, last_id);
        let transactions = vec![tr0, tr1];
        assert!(verify_transactions(&transactions));
    }
}

#[cfg(all(feature = "unstable", test))]
mod bench {
    extern crate test;
    use self::test::Bencher;
    use transaction::*;

    #[bench]
    fn verify_signatures_bench(bencher: &mut Bencher) {
        let alice_keypair = KeyPair::new();
        let last_id = Hash::default();
        let transactions: Vec<_> = (0..64)
            .into_par_iter()
            .map(|_| {
                let rando_pubkey = KeyPair::new().pubkey();
                Transaction::new(&alice_keypair, rando_pubkey, 1, last_id)
            })
            .collect();
        bencher.iter(|| {
            assert!(verify_signatures(&transactions));
        });
    }
}
