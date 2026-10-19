#![no_std]

mod common;

use batman_mesh_rust::mac::Mac;
use common::{app_frame, mac, Simulation};

// Signal Node chain, (A<-->B<-->C) A and C can communicate with B but not eachother
const CHAIN: &[(usize, usize)] = &[(0, 1), (1, 2)];

// Time before we start to send the messages so we don't start sending before nodes know
// of eachother
const WARM_UP_SECONDS: u64 = 30;

// How many frames each node transmitted since `before`
fn tx_since(sim: &mut Simulation<3>, before: [u32; 3]) -> [u32; 3] {
    [
        sim.tx_count(0) - before[0],
        sim.tx_count(1) - before[1],
        sim.tx_count(2) - before[2],
    ]
}

fn tx_now(sim: &mut Simulation<3>) -> [u32; 3] {
    [sim.tx_count(0), sim.tx_count(1), sim.tx_count(2)]
}


// --- Routing ----
#[test]
fn chain_routes_through_the_middle() {
    let (a, b, c) = (mac(0x0A), mac(0x0B), mac(0x0C));
    let mut sim: Simulation<3> = Simulation::new([a, b, c], CHAIN);

    sim.run_for_seconds(80);

    // B hears both ends directly
    sim.assert_route(1, a, a);
    sim.assert_route(1, c, c);

    // A and C reach each other only through B
    sim.assert_route(0, a, b);
    sim.assert_route(2, c, b);

    // Direct links converge to full quality
    sim.assert_tq_at_least(0, b, 240);
    sim.assert_tq_at_least(2, b, 240);

    // Two-hop routes carry B's hop penalty: about 225 with a penalty of 30
    sim.assert_tq_at_least(0, a, 200);
    sim.assert_tq_at_least(2, c, 200);

    // check 
    let a_to_b = sim.route_tq(0, b);
    let a_to_c = sim.route_tq(0, c);
    assert!(
        a_to_c < a_to_b,
        "A's TQ to C ({}) should be below its TQ to B ({}): one hop must beat two",
        a_to_c,
        a_to_b
    );

    
    let c_to_a = sim.route_tq(2, a);
    let c_to_b = sim.route_tq(2, b);

    assert!(
        c_to_a < c_to_b,
        "C's TQ to A ({}) should be below its TQ to B ({}): one hop must beat two",
        c_to_a,
        c_to_b
    );

    // No assert_no_drops: when C forwards A's OGM, B hears it with itself
    // as prev_sender and drops it. That is the protocol working.
    sim.assert_no_drops();

}

// ---------------------------------------------------------------------------
// Unicast across the middle node
// ---------------------------------------------------------------------------

/// A unicast from A to C must arrive at C exactly once and unchanged. A
/// cannot hear C, so it can only get there by B forwarding it.
#[test]
fn unicast_crosses_the_middle_node() {
    let (a, b, c) = (mac(0x0A), mac(0x0B), mac(0x0C));
    let mut sim: Simulation<3> = Simulation::new([a, b, c], CHAIN);

    sim.run_for_seconds(WARM_UP_SECONDS);

    // Data can only flow once the route exists.
    sim.assert_route(0, c, b);

    let tx_before = tx_now(&mut sim);
    let drops_before = sim.drop_count;

    let frame = app_frame(c, a);
    sim.send(0, &frame);

    // Delivered at C, unchanged, and nowhere else.
    let at_c = sim.delivered(2);
    assert_eq!(at_c.len(), 1, "C should receive the unicast exactly once");
    assert_eq!(at_c[0].bytes(), &frame[..], "C received a different frame than A sent");

    assert_eq!(
        sim.delivered(1).len(),
        0,
        "B must forward the unicast, not deliver it to its own app"
    );
    assert_eq!(sim.delivered(0).len(), 0, "A must not receive its own unicast");

    // A sends once, B forwards once, C only receives.
    let tx = tx_since(&mut sim, tx_before);
    assert_eq!(tx[0], 1, "A should transmit the unicast once");
    assert_eq!(tx[1], 1, "B should forward the unicast exactly once");
    assert_eq!(tx[2], 0, "C must not transmit anything for a unicast addressed to it");

    // A unicast has no reason to be dropped anywhere along the way.
    assert_eq!(
        sim.drop_count, drops_before,
        "the unicast caused drops, last: {:?}",
        sim.last_drops
    );
}

// ---------------------------------------------------------------------------
// Broadcast from an end of the chain
// ---------------------------------------------------------------------------

/// A broadcast from A must reach both B and C exactly once and unchanged,
/// and every node must transmit it exactly once: A sends it, B and C each
/// rebroadcast it once.
#[test]
fn broadcast_from_an_end_reaches_everyone_once() {
    let (a, b, c) = (mac(0x0A), mac(0x0B), mac(0x0C));
    let mut sim: Simulation<3> = Simulation::new([a, b, c], CHAIN);

    sim.run_for_seconds(WARM_UP_SECONDS);

    // C drops broadcasts from originators it does not know, and it can
    // only know A through B.
    sim.assert_route(2, a, b);

    let tx_before = tx_now(&mut sim);

    let frame = app_frame(Mac::BROADCAST, a);
    sim.send(0, &frame);

    for (i, name) in [(1, "B"), (2, "C")] {
        let got = sim.delivered(i);
        assert_eq!(got.len(), 1, "{} should receive the broadcast exactly once", name);
        assert_eq!(got[0].bytes(), &frame[..], "{} received a different frame than A sent", name);
    }
    assert_eq!(sim.delivered(0).len(), 0, "A must not receive its own broadcast back");

    // Flooding: every node transmits the broadcast exactly once.
    let tx = tx_since(&mut sim, tx_before);
    assert_eq!(tx[0], 1, "A should send the broadcast once");
    assert_eq!(tx[1], 1, "B should rebroadcast it exactly once");
    assert_eq!(tx[2], 1, "C should rebroadcast it exactly once");

    // No drop check: A hears B's rebroadcast and drops its own broadcast,
    // and B hears C's rebroadcast and drops it as a duplicate. Both correct.
}

// ---------------------------------------------------------------------------
// Broadcast from the middle of the chain
// ---------------------------------------------------------------------------

/// A broadcast from B must reach A and C exactly once each, from a single
/// transmission: one frame from B is heard by both ends at the same time.
#[test]
fn broadcast_from_the_middle_reaches_both_ends_in_one_transmission() {
    let (a, b, c) = (mac(0x0A), mac(0x0B), mac(0x0C));
    let mut sim: Simulation<3> = Simulation::new([a, b, c], CHAIN);

    sim.run_for_seconds(WARM_UP_SECONDS);

    // A and C must know B, or they drop its broadcast as unknown.
    sim.assert_route(0, b, b);
    sim.assert_route(2, b, b);

    let tx_before = tx_now(&mut sim);

    let frame = app_frame(Mac::BROADCAST, b);
    sim.send(1, &frame);

    for (i, name) in [(0, "A"), (2, "C")] {
        let got = sim.delivered(i);
        assert_eq!(got.len(), 1, "{} should receive the broadcast exactly once", name);
        assert_eq!(got[0].bytes(), &frame[..], "{} received a different frame than B sent", name);
    }
    assert_eq!(sim.delivered(1).len(), 0, "B must not receive its own broadcast back");

    // One transmission from B reaches both ends at once. A and C then each
    // rebroadcast once; only B hears those, and drops them as its own.
    let tx = tx_since(&mut sim, tx_before);
    assert_eq!(
        tx[1], 1,
        "B should need a single transmission to reach both A and C"
    );
    assert_eq!(tx[0], 1, "A should rebroadcast it exactly once");
    assert_eq!(tx[2], 1, "C should rebroadcast it exactly once");
}