//! The chunk's name pools: interned identifier atoms (with a dedicated entry
//! per frame-resolved site, see `name_site`) and private-name spellings.

use super::{CompileError, CompileGoal, Compiler};

/// Name pools up to this size are searched linearly; a larger one gets an index.
const NAME_SCAN_LIMIT: usize = 16;

impl Compiler {
    // `Atom` hashes and compares by identity, never through its `OnceCell`.
    #[allow(clippy::mutable_key_type)]
    pub(super) fn add_name(&mut self, name: &str) -> Result<u16, CompileError> {
        if let Some(index) = &self.name_index {
            let atom = crate::interpreter::Atom::intern(name);
            if let Some(&i) = index.get(&atom) {
                return Ok(i);
            }
            return self.push_name(atom);
        }
        if let Some(i) = self
            .names
            .iter()
            .enumerate()
            .position(|(i, n)| n.as_str() == name && !self.is_frame_site(i as u16))
        {
            return Ok(i as u16);
        }
        self.push_name(crate::interpreter::Atom::intern(name))
    }

    pub(super) fn is_frame_site(&self, idx: u16) -> bool {
        self.frame_sites.binary_search(&idx).is_ok()
    }

    /// The name-pool index for a bare-name reference site. Inside a Script's
    /// block scope, a name no open block scope declares can only resolve in
    /// the frame (global) environment, so its site gets a dedicated entry whose
    /// hint starts the scope walk there instead of verifying every block scope
    /// in between. Block scopes are the only environments a chunk opens, and
    /// `scopes` lists everything each declares.
    pub(super) fn name_site(&mut self, name: &str) -> Result<u16, CompileError> {
        if self.goal != CompileGoal::Script
            || self.scopes.is_empty()
            || self
                .scopes
                .iter()
                .any(|names| names.iter().any(|n| n == name))
        {
            return self.add_name(name);
        }
        if let Some(&idx) = self
            .frame_sites
            .iter()
            .find(|&&i| self.names[usize::from(i)].as_str() == name)
        {
            return Ok(idx);
        }
        let idx = u16::try_from(self.names.len())
            .map_err(|_| CompileError::Ineligible("name pool overflow"))?;
        self.names.push(crate::interpreter::Atom::intern(name));
        self.frame_sites.push(idx);
        Ok(idx)
    }

    #[allow(clippy::mutable_key_type)]
    fn push_name(&mut self, atom: crate::interpreter::Atom) -> Result<u16, CompileError> {
        let idx = self.names.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("name pool overflow"));
        }
        if self.name_index.is_none() && idx >= NAME_SCAN_LIMIT {
            self.name_index = Some(
                self.names
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| !self.is_frame_site(i as u16))
                    .map(|(i, n)| (n.clone(), i as u16))
                    .collect(),
            );
        }
        if let Some(index) = &mut self.name_index {
            index.insert(atom.clone(), idx as u16);
        }
        self.names.push(atom);
        Ok(idx as u16)
    }

    /// Index of a private name's source spelling in the chunk's private-name
    /// pool. The VM resolves each entry to its Private Name once per frame.
    pub(super) fn add_private_name(&mut self, name: &str) -> Result<u16, CompileError> {
        if let Some(i) = self.private_names.iter().position(|n| n == name) {
            return Ok(i as u16);
        }
        let idx = self.private_names.len();
        if idx > u16::MAX as usize {
            return Err(CompileError::Ineligible("private name pool overflow"));
        }
        self.private_names.push(name.to_string());
        Ok(idx as u16)
    }
}
