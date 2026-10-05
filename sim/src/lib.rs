//! Deterministic sim core. No clock, net, fs, threads. See design/11.
pub mod deck;
pub mod error;
pub mod ids;
pub mod ledger;
pub mod net;
pub mod pos;
pub mod renderer;
pub mod scheduler;
pub mod world;

pub use error::SimError;
pub use deck::{CellPos, Host};
pub use ledger::{Ledger, Mailbox, MAIL_CAP, SPAM_CAP_PER_HOST_DAY};
pub use net::{LinkKind, NetGraph, NetKind, Route};
pub use ids::{EntityId, HostId, TileId};
pub use pos::Pos;
pub use renderer::{DeckView, NetView, RenderView, Renderer, StreetView};
pub use scheduler::Scheduler;
pub use world::{Intent, Delta, Event, World};
