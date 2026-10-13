//! Deterministic sim core. No clock, net, fs, threads. See design/11.
pub mod civic;
pub mod crime;
pub mod data;
pub mod deck;
pub mod error;
pub mod ids;
pub mod ledger;
pub mod mailer;
pub mod net;
pub mod portal;
pub mod pos;
pub mod power;
pub mod renderer;
pub mod scheduler;
pub mod shop;
pub mod street;
pub mod talk;
pub mod veil;
pub mod world;

pub use civic::{cop_duty, role_table, Activity, Role, Timetable};
pub use crime::{
    trace_evidence, Act, Alignment, Clans, Crime, License, Record, Standing, LICENSE_KILLS,
    WARRANT_AT,
};
pub use data::{
    parse_cards, parse_dialogues, parse_districts, parse_jobs, parse_vulns, CardDef, CardTable,
    DistrictDef, JobDef, VulnDef,
};
pub use deck::{CellPos, Host};
pub use error::SimError;
pub use ids::{EntityId, HostId, TileId};
pub use ledger::{Ledger, Mailbox, MAIL_CAP, SPAM_CAP_PER_HOST_DAY};
pub use mailer::{Mailer, ZombieNet};
pub use net::{LinkKind, NetGraph, NetKind, Route};
pub use portal::{Portal, PortalNet};
pub use pos::Pos;
pub use power::PowerGrid;
pub use renderer::{DeckView, NetView, RenderView, Renderer, StreetView};
pub use scheduler::Scheduler;
pub use shop::{Shop, Stock};
pub use street::{gen_section, window, District, GuaranteeKind, Ports, Rng, Section, SECTION_SIZE};
pub use talk::{Choice, Conversation, Dialogue, Node};
pub use veil::{Veil, DEATH_DRAIN, ENTER_COST, KICK_DRAIN};
pub use world::{Delta, Dir, Event, Intent, Verb, World};
