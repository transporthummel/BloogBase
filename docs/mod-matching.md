# Matching mods

A mod added by hand is just a folder or ZIP. BloogBase tries to tell what it
is: an index entry's download, a level pack for another mod, or a new
version of a mod already installed. This page covers those matches. How a
mod starts is in [recipes.md](recipes.md), which game copy it goes on in
[keenwiki.md](keenwiki.md).

## Name and details from the archive name

Archives from the KeenWiki file server carry metadata in their name
(`guess_metadata` in `src-tauri/src/mods.rs`):

| Pattern                                      | Example                                              |
| -------------------------------------------- | ---------------------------------------------------- |
| `m.<author>.<name>[_v<version>].<ep>.<date>` | `m.dave216.aurora_bliss_v.1.2.k4.2023-08-23`         |
| `l.<author>.<ep>.<date>`                     | a level pack without a name                          |
| `l.<author>.<mod>[_v<version>].<date>`       | `l.keen_community.episode_smile.…`: levels for a mod |

`<ep>` is `k1` to `k6` or `kd`, `<date>` is `YYYY-MM-DD`. The name gets its
underscores turned into spaces and title case, the version is split off
(`_v` or `.v`), and the episode serves as a hint for the game
([recipes.md](recipes.md)). A level pack without the episode names the mod
its levels are for (`made_for`). Other archives are named after their
folder or file name.

## An index entry

The index has tested recipes, so a mod that is an index entry's download
should be installed as that entry. A dropped or picked archive matches an
entry of the same game whose **newest** download has the archive's SHA-256.
Older downloads don't count here: the entry's recipe may not fit them. The
dialog then offers the entry instead ([keenwiki.md](keenwiki.md) has the
same check for KeenWiki links).

After any install, installed mods that aren't tied to an entry are tied to
the one whose download they were installed from, byte for byte, older
downloads included. A mod installed from KeenWiki before its entry joined
the index thus becomes that entry's, and the entry offers its update.

## The mod a mod goes over

Some mods are made to be copied over another one: Lego Levels over Lego
Keen, level packs over Episode Smile. They're played over that mod
(`extends`): its files below, theirs on top, with its recipe if they have
none of their own ([patching.md](patching.md)).

Candidates are the index entries and installed mods of the same game that
don't go over another mod themselves.

1. **Missing files.** A way to start the mod that loads files it lacks
   (`%patchfile`), or levels whose readme names a start the mod lacks
   ([readmes.md](readmes.md)), needs a mod that has them:
   - first an index entry whose `provides` lists them all, with its
     installed mod if there is one,
   - else an installed mod that has them all in its files.
2. **Levels alone.** A mod of levels only, with nothing missing, goes over
   the mod its archive name says (`made_for`), compared by letters and
   digits only, ignoring case: `episode_smile` finds "Episode Smile".

The dialog says which mod it found and plays the new one over it unless
the player unticks that. If the mod below isn't installed yet but is in the
index, it's installed first. Without a match, the dialog names the missing
files and links the readme.

Removing a mod removes the mods played over it too.

## A new version of an installed mod

The dialog asks whether to replace an installed mod or install alongside,
when it finds one for the same game that

- comes from the same KeenWiki page, or
- has the same name (as typed or as detected, case and surrounding spaces
  aside), unless their KeenWiki pages or index entries are both known and
  differ: then they're different mods, whatever they're called.

An update check ([keenwiki.md](keenwiki.md)) opens the dialog with the old
mod already chosen.

### Replacing

Replacing installs the new files in place of the old ones under the same
id:

- The new files are staged, then swapped with the old ones; if that fails,
  the old ones are put back.
- The profile, its saves and backups stay. The composed folder is rebuilt
  on the next launch, and cover and readme are read again.
- Details the new files can't tell are kept from the old version: author
  links, description, KeenWiki page, homepage, forum threads.
- The new version goes on the old one's copy of the game if it fits.
- A running game blocks the replace, so files never change under it.

Index installs work the same way: installing an entry that's already
installed updates it in place.
