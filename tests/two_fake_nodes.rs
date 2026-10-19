//! Two nodes on one link.
//!
//! Run with: cargo test --test two_fake_nodes

#![no_std]

mod common;

use common::*;
use batman_mesh_rust::mac::Mac;

/// Both nodes must learn each other, route directly, and after 80 seconds
/// of perfect delivery reach (nearly) full link quality.
#[test]
fn two_nodes_learn_each_other() {
    let a = mac(0x0A);
    let b = mac(0x0B);

    // Node 0 is A, node 1 is B, and they can hear each other.
    let mut sim: Simulation<2> = Simulation::new([a, b], &[(0, 1)]);

    // Long enough for the 64-OGM windows to fill.
    sim.run_for_seconds(80);

    sim.assert_route(0, b, b);
    sim.assert_route(1, a, a);

    // Full windows on a perfect link give a TQ of 255. Anything well below
    // means the echo offset, the windows or the TQ formula is off, even
    // though routing works.
    sim.assert_tq_at_least(0, b, 240);
    sim.assert_tq_at_least(1, a, 240);

    sim.assert_no_drops();
}

/// The same two nodes without a link. Proves that assert_route really fails
/// when nothing is learned, so the test above cannot pass by accident.
#[test]
#[should_panic(expected = "never learned")]
fn unlinked_nodes_never_learn_each_other() {
    let a = mac(0x0A);
    let b = mac(0x0B);


    let mut sim: Simulation<2> = Simulation::new([a, b], &[]);

    sim.run_for_seconds(10);

    sim.assert_route(0, b, b);
}


#[test]
fn unicast_deliverd(){
    let a = mac(0x0A);
    let b = mac(0x0B);
 
    let mut sim: Simulation<2> = Simulation::new([a, b], &[(0, 1)]);
 
    // Give the OGMs time to build a route from A to B.
    sim.run_for_seconds(20);
 
    let frame = app_frame(b, a);
    sim.send(0, &frame);
 
    let at_b = sim.delivered(1);
    assert_eq!(at_b.len(), 1, "B should receive the unicast exactly once");
    assert_eq!(at_b[0].bytes(), &frame[..], "B received a different frame than A sent");
 
    assert_eq!(sim.delivered(0).len(), 0, "A must not receive its own unicast");
 
    sim.assert_no_drops();
}

#[test]
fn unicast_without_a_route_is_refused(){
    let a = mac(0x0A);
    let b = mac(0x0B);
 
    let mut sim: Simulation<2> = Simulation::new([a, b], &[(0, 1)]);
 
    // No time has passed: A has not heard a single OGM from B.
    let frame = app_frame(b, a);
    assert!(
        !sim.try_send(0, &frame),
        "A has no route to B yet, so sending must fail"
    );
 
    assert_eq!(sim.delivered(1).len(), 0, "nothing may reach B without a route");
}

#[test]
fn broadcast_is_delivered_once() {
    let a = mac(0x0A);
    let b = mac(0x0B);
 
    let mut sim: Simulation<2> = Simulation::new([a, b], &[(0, 1)]);
 
    // B drops broadcasts from originators it does not know yet, so let the
    // OGMs introduce A to B first.
    sim.run_for_seconds(20);
 
    let frame = app_frame(Mac::BROADCAST, a);
    sim.send(0, &frame);
 
    let at_b = sim.delivered(1);
    assert_eq!(at_b.len(), 1, "B should receive the broadcast exactly once");
    assert_eq!(at_b[0].bytes(), &frame[..], "B received a different frame than A sent");
 
    assert_eq!(
        sim.delivered(0).len(),
        0,
        "A must not receive its own broadcast back"
    );
 
    // No assert_no_drops here: B rebroadcasts the frame, A hears its own
    // broadcast come back and drops it. That drop is correct behaviour.
}