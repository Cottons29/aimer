use aimer_wasm_guest::anteros::Version;
use crate::guest_support::{FullStateProgram, GuestVariant, reject_migration};
pub struct InitialBuildTrapVariant;
impl GuestVariant for InitialBuildTrapVariant {
    const SCHEMA_VERSION: Version = Version::new(2, 0);
    const STATE_TAG: u8 = 2;
    const CALLBACK_STEP: u8 = 10;
    const HEADER: &'static str = "UNREACHABLE BUILD TRAP";
    const BUTTON: &'static str = "unreachable";
    const TRAP_ON_BUILD: bool = true;
    fn migrate(previous: Version, payload: &[u8]) -> Option<Vec<u8>> { reject_migration(previous, payload) }
}
pub type FullStateGuest = FullStateProgram<InitialBuildTrapVariant>;
