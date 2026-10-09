mod common;

use common::{ROCK, SAND, world};

#[test]
fn equal_states_hash_equal_and_hash_ignores_frame_counter() {
    let (mut a, mut b) = (world(2, 2, 4), world(2, 2, 4));
    a.set(1, 1, ROCK).unwrap();
    b.set(1, 1, ROCK).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
    a.step();
    b.step();
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn single_cell_change_changes_hash() {
    let a = world(2, 2, 4);
    let mut b = a.clone();
    b.set(7, 7, SAND).unwrap();
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn hash_is_independent_of_chunk_size() {
    let (mut a, mut b) = (world(2, 2, 4), world(1, 1, 8));
    for w in [&mut a, &mut b] {
        w.set(3, 5, SAND).unwrap();
        w.set(6, 2, ROCK).unwrap();
    }
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn hash_depends_on_dimensions() {
    assert_ne!(world(2, 1, 4).state_hash(), world(1, 2, 4).state_hash());
}
