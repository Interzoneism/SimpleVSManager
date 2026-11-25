//! Read-only check against the real cloud: lists public modlists and subscription counts.
//! Uses (never changes) `<manager dir>/firebase-auth.json`; refuses to run without one so it can
//! never create a new account. `cargo run -p svsm-core --example cloud_live -- <manager dir>`

use svsm_core::cloud::{auth, Authenticator, CloudConfig, CloudStore};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: cloud_live <manager dir>"));
    if auth::read_identity(&dir.join(auth::IDENTITY_FILE)).is_none() {
        eprintln!("No firebase-auth.json in {}; not signing up a new account.", dir.display());
        std::process::exit(2);
    }
    let a = Authenticator::new(&dir, None, CloudConfig::default()).unwrap();
    let tmp = std::env::temp_dir().join("svsm3-cloud-live-cache");
    let store = CloudStore::new(&a, CloudConfig::default(), &tmp).unwrap();
    let list = store.registry().await.expect("registry");
    println!("{} public modlists; newest: {:?} (revision {})", list.len(), list.first().and_then(|e| e.name.clone()), list.first().map(|e| e.revision).unwrap_or(0.0));
    match store.subscriptions().await {
        Ok(s) => println!("subscriptions readable: {} modlists have subscribers", s.counts.len()),
        Err(e) => println!("subscriptions not readable: {e}"),
    }
}
