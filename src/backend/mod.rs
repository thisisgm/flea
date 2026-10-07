pub mod listing;
pub mod aliases;
pub mod archive;
pub mod archivelist;
pub mod archiveops;
pub mod archivespec;
pub mod archiveuser;
pub mod archivereq;
pub mod archivework;
pub mod mime;
pub mod fsinfo;
pub mod fsname;
pub mod fsinforeq;
pub mod extclass;
pub mod netfs;
pub mod durable;
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
pub mod gvfslist;
pub mod fuzzy;
// The path bar's folder jump; see docs/protocol.md "jump".
pub mod jump;
pub mod search;
pub mod searchreq;
pub mod shebang;
pub mod sort;
pub mod ordering;
pub mod state;
pub mod mediaprobe;
pub mod meta;
pub mod metareq;
pub mod metasort;
pub mod owner;
pub mod peek;
pub mod pdfcopy;
pub mod proto;
mod providers;
// Directive 71: the LocalSend row's peers and its send, driven against localsend-cli on a pty.
pub mod localsend;
// The same CLI's own drawing, read back into rows and sentences.
pub mod localsendtext;
pub mod permissions;
pub mod picker;
pub mod menu_actions;
mod menu_slot;
mod menu_registry;
mod menudelete;
pub mod trashbrowse;
pub mod trashdelete;
pub mod trashmanifest;
pub mod rows;
pub mod rowguard;
pub mod run;
pub mod shelfdrop;
pub mod timing;
pub mod md5;
pub mod thumbspec;
pub mod thumbargv;
pub mod thumbcache;
pub mod sandbox;
pub mod jail;
pub mod child;
pub mod thumbs;
pub mod thumbreq;
pub mod thumbwrite;
// The pre-linked video thumbnailer and the backend's link to it; see AGENTS.md "Thumbnail worker".
pub mod fdpass;
pub mod thumbworker;
pub mod workerlink;
// File operations and the undo journal they record into.
pub mod collide;
pub mod convert;
pub mod copyfile;
pub mod copymanifest;
pub mod manifestdir;
pub mod copynode;
pub mod ops;
pub mod opscancel;
pub mod opsdispatch;
pub mod opsreq;
pub mod movebatch;
pub mod iomount;
mod mountinfo;
mod renamecompat;
pub mod trash;
pub mod undo;
pub mod undoshare;
pub mod undocodec;
mod undorebase;
mod undostage;
// Test-only: per-thread work counters the shared journal's count tests read.
#[cfg(test)]
pub(crate) mod undoprobe;
#[cfg(test)]
mod undocost_tests;
#[cfg(test)]
mod undostage_tests;
// Test-only: a kernel network mount's EINVAL on RENAME_NOREPLACE, routed to a plain rename and never a copy.
#[cfg(test)]
mod renamecompat_tests;
pub mod redo;
pub mod link;
// The open listing's directory, watched so an outside change reaches the client; see docs/protocol.md "changed".
pub mod watch;
// The walk over a buffer of inotify events, shared by both watches.
pub mod inotifyburst;
// The thread behind the peek watch, stoppable by its owner.
pub mod peekpump;
// The directories the columns view peeks at, watched the same way; see docs/protocol.md "changed".
pub mod peekwatch;
// Network folders inotify cannot see, re-statted at a named interval for the open folder only.
pub mod watchpoll;
// The system clipboard for files: set, get, clear and watch beside the request loop.
pub mod clipreq;
// Test-only: the failing-first manifest behaviour for undo of a failed tree copy.
#[cfg(test)]
mod undomanifest_tests;
// Test-only: the copy manifest under a failing filesystem, each cap in a re-executed child.
#[cfg(test)]
mod manifestfsize_tests;
// Test-only: hard rule 9's sandbox root, so no destructive test names a path outside one.
#[cfg(test)]
pub mod testdir;
// Test-only: the fifo, writer and bound every hang test shares.
#[cfg(test)]
pub mod fifotest;
// Test-only: the one probe that says whether this box can actually run the bwrap jail.
#[cfg(test)]
pub mod sandboxprobe;

pub mod dirsizeworker;
