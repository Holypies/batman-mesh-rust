#![cfg_attr(not(test), no_std)]

// Crate-internal helpers
mod arrayvec;
mod bytes;

// Public API
pub mod eth;
pub mod framebuffer;
pub mod hardif;
pub mod mac;
pub mod meshctx;
pub mod originator;
pub mod packets;
pub mod consts;
pub mod time;
pub mod seqno;

pub use framebuffer::{FrameBuffer, FrameError};
pub use hardif::{HardInterface, InterfaceError, LinkDriver};
pub use mac::Mac;
pub use meshctx::MeshCtx;
pub use packets::PacketError;