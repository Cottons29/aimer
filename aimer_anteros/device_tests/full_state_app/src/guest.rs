use aimer_wasm_guest::anteros::Version;
use crate::guest_support::{FullStateProgram, GuestVariant, reject_migration};
pub struct InitialVariant;
impl GuestVariant for InitialVariant {
    const SCHEMA_VERSION: Version = Version::new(1, 0);
    const STATE_TAG: u8 = 1;
    const CALLBACK_STEP: u8 = 1;
    const HEADER: &'static str = "FULL STATE / INITIAL";
    const BUTTON: &'static str = "increment +1";
    const TRAP_ON_BUILD: bool = false;
    fn migrate(previous: Version, payload: &[u8]) -> Option<Vec<u8>> { reject_migration(previous, payload) }
}
pub type FullStateGuest = FullStateProgram<InitialVariant>;
