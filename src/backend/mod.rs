pub mod listing;
pub mod aliases;
pub mod archive;
pub mod archiveenc;
pub mod archivefree;
pub mod archivelist;
pub mod archiveops;
pub mod archivepass;
pub mod archivespec;
pub mod archivereq;
pub mod archivework;
pub mod appicon;
pub mod mime;
pub mod fsinfo;
pub mod fsname;
pub mod fsinforeq;
pub mod extclass;
pub mod gvfsroot;
pub mod netfs;
pub mod durable;
pub mod icons;
pub mod regfile;
pub mod exiffacts;
pub mod imagesize;
pub mod kind;
pub mod linecount;
pub mod dirsize;
pub mod dirsizereq;
pub mod listpaths;
// Issue 144: a remote list scans off the event loop; see src/backend/listwork.rs.
pub mod listwork;
pub mod unmount;
pub mod events;
pub mod scan;
pub mod gvfslist;
pub mod fuzzy;
// The path bar's folder jump; see docs/protocol.md "jump".
pub mod jump;
// A drop target's device, stat'd off the loop; see docs/protocol.md "devs".
pub mod devsreq;
// The file kind of each path named, symlinks followed, for the FAVORITES heading's pin drop; see docs/protocol.md "kind".
pub mod kindreq;
pub mod search;
pub mod searchindex;
pub mod searchsession;
pub mod searchtags;
pub mod searchwalk;
#[cfg(test)]
pub(crate) mod searchwalkhold;
pub mod searchreq;
pub mod searchhold;
pub mod searchpatch;
pub mod answersizes;
pub mod shebang;
pub mod sgr;
pub mod codereq;
pub mod htmlreq;
mod htmlcache;
mod htmldeps;
mod htmlsandbox;
pub mod compare;
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
pub mod officepdf;
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
pub(crate) mod menu_registry;
mod menu_registry_work;
mod menucontents;
mod menufacts;
mod walkmounts;
mod menudelete;
pub mod trashbrowse;
pub mod trashland;
pub mod journalgate;
pub mod trashdelete;
pub mod trashmanifest;
pub mod rows;
pub mod rowguard;
// Dolphin-compatible file tags in user.xdg.tags, read per viewport row and never swept.
pub mod tags;
pub mod tagsreq;
pub mod tagsundo;
pub mod filegen;
pub mod run;
pub mod shelfdrop;
pub mod timing;
pub mod md5;
pub mod thumbspec;
pub mod thumbargv;
pub mod thumbbind;
pub mod thumbcache;
pub mod sandbox;
pub mod jail;
// The bounded reap kill_tree ends a jail with, so a child the kernel holds never holds a caller; see jail.rs.
pub mod jailreap;
pub mod child;
pub mod thumbs;
pub mod thumbreq;
pub mod thumbdefer;
// Test-only: a request wider than the pool's queue waits for a worker rather than being answered empty.
#[cfg(test)]
mod thumbdefer_tests;
pub mod thumbwrite;
// The pre-linked video thumbnailer and the backend's link to it; see AGENTS.md "Thumbnail worker".
pub mod fdpass;
pub mod thumbworker;
pub mod workerlink;
// File operations and the undo journal they record into.
pub mod collide;
pub mod collidelink;
pub mod merge;
pub mod mergeapply;
pub mod fatnames;
pub mod pasteimage;
pub mod convert;
// Quick Look's font page: the facts from the font's tables and the sample job; see src/backend/fontinfo/render.rs.
pub mod fontinfo;
pub mod copyfile;
pub mod copymanifest;
pub mod manifestdir;
pub mod copynode;
mod copyread;
pub mod unheld;
pub mod ops;
pub mod opsname;
pub mod opscancel;
pub mod opsdispatch;
pub mod opsreq;
pub mod bulkrename;
pub mod movebatch;
pub mod iomount;
pub mod mountinfo;
mod renamecompat;
pub mod trash;
pub mod hometrash;
pub(crate) mod trashdate;
pub(crate) mod homerestore;
pub mod undo;
pub mod undoshare;
pub mod undocodec;
mod undorebase;
mod undostage;
// The image flasher's backend: eligibility, probe, write loop and helper.
pub mod flash;
pub mod flashdesc;
pub(crate) mod flashdisk;
pub mod flasheject;
pub mod flasheligible;
// Test-only: the fake /sys trees the flasher tests share.
#[cfg(test)]
pub(crate) mod flashfake;
// Test-only: the disk chains the system, a swap and a mount rest on.
#[cfg(test)]
mod flashdisk_tests;
// Test-only: the write loop, verify and wipe against an O_DIRECT target.
#[cfg(test)]
mod flashdirect_tests;
pub mod flashhelper;
pub(crate) mod flashio;
pub mod flashimage;
pub mod flashmeter;
pub mod flashop;
pub mod flashpipe;
pub(crate) mod flashpump;
// The bench seam exists only with the flash-test-seam feature, so a release build holds no symbol of it.
#[cfg(feature = "flash-test-seam")]
pub(crate) mod flashseam;
pub mod flashslot;
pub mod flashverify;
pub(crate) mod flashzstd;
pub mod flashwrite;
pub mod xxh64;
// Test-only: per-thread work counters the shared journal's count tests read.
#[cfg(test)]
pub(crate) mod undoprobe;
#[cfg(test)]
mod undocost_tests;
// Test-only: per-thread work counters the paste cost test reads.
#[cfg(test)]
pub(crate) mod opsprobe;
// Test-only: the work one paste of many small files costs.
#[cfg(test)]
mod opscopymany_tests;
#[cfg(test)]
mod undostage_tests;
// Test-only: a kernel network mount's EINVAL on RENAME_NOREPLACE, routed to a plain rename and never a copy.
#[cfg(test)]
mod renamecompat_tests;
// Test-only: the closing progress line a paced transfer sends after its last item.
#[cfg(test)]
mod transferclose_tests;
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
// Test-only: the host's process table, so a test can follow a jailed child and everything below it.
#[cfg(test)]
pub mod procwatch;
// Test-only: holds one path's directory read or mode stat, so a test can prove which thread made the call.
#[cfg(test)]
pub mod scanhold;
// Test-only: keeps the deliberate worker panics quiet without any test swapping the process-wide hook.
#[cfg(test)]
pub mod quietpanic;
// Test-only: holds a child's reap as an uninterruptible sleep would, so kill_tree's bound is pinned.
#[cfg(test)]
pub mod reaphold;

pub mod dirsizeworker;
// The decode-free RAW container walker: the largest embedded JPEG preview plus EXIF facts.
pub mod rawembed;
pub mod rawtiff;
pub mod rawnote;
pub mod rawbmff;
pub mod rawraf;
pub mod rawexif;
pub mod rawpreview;
