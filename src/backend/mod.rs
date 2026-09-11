pub mod listing;
pub mod aliases;
pub mod archive;
pub mod archivelist;
pub mod archiveops;
pub mod archivespec;
pub mod archivereq;
pub mod archivework;
pub mod mime;
pub mod fsinfo;
pub mod icons;
pub mod regfile;
pub mod imagesize;
pub mod kind;
pub mod linecount;
pub mod dirsize;
pub mod dirsizereq;
pub mod listpaths;
pub mod events;
pub mod scan;
pub mod fuzzy;
pub mod search;
pub mod searchreq;
pub mod sort;
pub mod ordering;
pub mod state;
pub mod mediaprobe;
pub mod meta;
pub mod metareq;
pub mod metasort;
pub mod owner;
pub mod peek;
pub mod proto;
mod providers;
pub mod permissions;
pub mod picker;
pub mod menu_actions;
mod menu_registry;
mod menudelete;
pub mod trashbrowse;
pub mod trashdelete;
pub mod trashmanifest;
pub mod rows;
pub mod run;
pub mod md5;
pub mod thumbspec;
pub mod thumbargv;
pub mod thumbcache;
pub mod sandbox;
pub mod child;
pub mod thumbs;
pub mod thumbreq;
pub mod thumbwrite;
// File operations and the undo journal they record into.
pub mod convert;
pub mod copyfile;
pub mod copynode;
pub mod ops;
pub mod opsdispatch;
pub mod opsreq;
mod mountinfo;
mod renamecompat;
pub mod trash;
pub mod undo;
pub mod redo;
// The open listing's directory, watched so an outside change reaches the client; see docs/protocol.md "changed".
pub mod watch;
// Test-only: hard rule 9's sandbox root, so no destructive test names a path outside one.
#[cfg(test)]
pub mod testdir;
// Test-only: the fifo, writer and bound every hang test shares.
#[cfg(test)]
pub mod fifotest;
// Test-only: the one probe that says whether this box can actually run the bwrap jail.
#[cfg(test)]
pub mod sandboxprobe;
