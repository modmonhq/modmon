use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use super::{STRANGER_FOR, STRANGERS, Strangers};

fn address(index: usize) -> Ipv4Addr {
    Ipv4Addr::from(0x7f4d_0000 + u32::try_from(index).unwrap_or(u32::MAX))
}

#[test]
fn a_full_list_of_strangers_forgets_its_oldest_for_a_new_one() {
    let base = Instant::now();
    let mut strangers = Strangers::default();
    for index in 0..STRANGERS {
        strangers.add_at(
            address(index),
            base + Duration::from_millis(1) * u32::try_from(index).unwrap_or(u32::MAX),
        );
    }
    assert_eq!(strangers.pinned.len(), STRANGERS);

    let later = base + Duration::from_secs(1);
    strangers.add_at(address(STRANGERS), later);
    assert_eq!(strangers.pinned.len(), STRANGERS);
    assert!(!strangers.pinned.contains_key(&address(0)));
    assert!(strangers.pinned.contains_key(&address(1)));
    assert_eq!(strangers.pinned.get(&address(STRANGERS)), Some(&later));

    strangers.add_at(address(5), later);
    assert_eq!(strangers.pinned.len(), STRANGERS);
    assert!(strangers.pinned.contains_key(&address(1)));
}

#[test]
fn strangers_past_their_time_make_room() {
    let base = Instant::now();
    let mut strangers = Strangers::default();
    for index in 0..STRANGERS {
        strangers.add_at(address(index), base);
    }
    strangers.add_at(address(STRANGERS), base + STRANGER_FOR);
    assert_eq!(strangers.pinned.len(), 1);
}
