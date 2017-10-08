extern crate leveldb_rs;
extern crate rand;

use leveldb_rs::DB;
use leveldb_rs::Options;
use rand::Rng;
use rand::AsciiGenerator;

use std::error::Error;
use std::iter::FromIterator;

const KEY_LEN: usize = 16;
const VAL_LEN: usize = 48;

fn gen_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    let mut rand = rng.gen_ascii_chars();

    String::from_iter(rand.take(len))
}

fn fill_db(db: &mut DB, entries: usize) -> Result<(), Box<Error>> {
    for i in 0..entries {
        let (k, v) = (gen_string(KEY_LEN), gen_string(VAL_LEN));
        db.put(k.as_bytes(), v.as_bytes())?;

        if i % 100 == 0 {
            db.flush()?;
        }
    }
    Ok(())
}

fn main() {
    let opt = Options::default();
    let mut db = DB::open("test1", opt).unwrap();

    fill_db(&mut db, 4096).unwrap();
}