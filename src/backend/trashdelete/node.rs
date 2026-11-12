use super::*;

impl Node {
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [
            self.identity.dev,
            self.identity.ino,
            self.identity.size,
            self.identity.mtime as u64,
            self.identity.nanos as u64,
            self.identity.mode as u64,
            self.children,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(self.relative.as_os_str().as_bytes());
        bytes
    }
    // Sample record: seven little-endian u64 fields followed by the raw relative filename bytes.
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, Refusal> {
        const HEADER: usize = 7 * 8;
        if bytes.len() < HEADER {
            return Err(Refusal::changed("Truncated Trash node review."));
        }
        let number = |index: usize| {
            u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap())
        };
        let relative = PathBuf::from(OsString::from_vec(bytes[HEADER..].to_vec()));
        if relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(Refusal::changed("Invalid relative Trash review path."));
        }
        Ok(Self {
            relative,
            identity: Identity {
                dev: number(0),
                ino: number(1),
                size: number(2),
                mtime: number(3) as i64,
                nanos: number(4) as i64,
                mode: number(5) as u32,
            },
            children: number(6),
        })
    }
    pub(super) fn directory(&self) -> bool {
        self.identity.mode & 0o170000 == 0o040000
    }
    fn deletion_identity_matches(&self, current: &Identity) -> bool {
        self.identity.matches_survivor(current)
    }
    pub(super) fn claim_remove(
        &self,
        parent: &File,
        name: &OsStr,
        quarantine: &File,
        record_offset: u64,
    ) -> Result<(), Refusal> {
        if !self.deletion_identity_matches(
            &identity(&fd_path(parent, name)).changed("Trash child unreadable")?,
        ) {
            return Err(Refusal::new(
                Outcome::ChildChanged,
                "Trash child changed; remaining data was preserved.",
            ));
        }
        let claimed = format!("entry-{}", record_offset);
        let claimed = OsStr::new(&claimed);
        rename(parent, name, quarantine, claimed).map_err(|reason| {
            Refusal::new(Outcome::ChildLeft, "Trash child could not be claimed").because(reason)
        })?;
        let path = fd_path(quarantine, claimed);
        let removal = (|| {
            if !self.deletion_identity_matches(
                &identity(&path).changed("Claimed Trash child unreadable")?,
            ) {
                return Err(Refusal::new(
                    Outcome::ChildChanged,
                    "Trash child changed while being claimed.",
                ));
            }
            if self.directory() {
                // remove_dir refuses arrivals absent from the snapshot instead of deleting them recursively.
                std::fs::remove_dir(&path)
                    .refuse(Outcome::ChildLeft, "Trash directory has surviving items")
            } else {
                std::fs::remove_file(&path)
                    .refuse(Outcome::ChildLeft, "Trash child could not be removed")
            }
        })();
        if let Err(error) = removal {
            // A child that cannot go back stays in the quarantine, where remove_empty finds it; the reason it could not is logged.
            if let Err(reason) = rename(quarantine, claimed, parent, name) {
                Refusal::new(
                    Outcome::ChildLeft,
                    format!("Trash child could not be put back: {reason}"),
                )
                .logged();
            }
            return Err(error);
        }
        Ok(())
    }
}
