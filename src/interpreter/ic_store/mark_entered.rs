use super::*;

impl StoreEntry {
    pub(super) fn mark_entered(&mut self, tick: u64) {
        self.last_used = tick;
        self.in_flight = self
            .in_flight
            .checked_add(1)
            .expect("IcStore in-flight count overflowed");
    }
}
