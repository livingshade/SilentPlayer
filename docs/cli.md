# Silent CLI

The generic CLI is Silent's third product target alongside macOS and iPhone. It uses the same
Rust application behavior as the Apple targets; it is not a database-editing substitute for the
app.

## Entry model

Simple process-level commands stay at the root:

```bash
silent --version
silent --help
```

Product commands cross an explicit CLI boundary:

```bash
silent --cli [global-options] <domain> <command> [arguments]
silent --cli --help
```

Stateful commands require both storage paths. Global options must appear before the domain and
cannot be repeated:

| Option | Meaning |
| --- | --- |
| `--db <path>` | Explicit SQLite library path |
| `--media-root <path>` | Explicit managed music directory |
| `--output table\|json` | Table output or stable JSON output |
| `--quiet` | Suppress successful output |
| `--yes` | Confirm destructive or overwriting operations |

There are no environment or current-directory storage defaults. Read-only stateful commands refuse
to create a missing database, so a typo cannot silently open an empty library. `library import`
and `library package import` are the only commands allowed to create one.

## Library

```bash
silent --cli library scan ~/Music
silent --cli --db ./silent.sqlite3 --media-root ./Music library import ~/Music song.flac
silent --cli --db ./silent.sqlite3 --media-root ./Music library list
silent --cli --db ./silent.sqlite3 --media-root ./Music library search "artist or title" --limit 25
silent --cli --db ./silent.sqlite3 --media-root ./Music library analyze
silent --cli --db ./silent.sqlite3 --media-root ./Music library audit
silent --cli --db ./silent.sqlite3 --media-root ./Music library package export ./Library.silentlibrary
silent --cli --db ./restored.sqlite3 --media-root ./RestoredMusic --yes library package import ./Library.silentlibrary
silent --cli --db ./silent.sqlite3 --media-root ./Music --yes library zero
```

`library import` accepts either one folder or one or more files. It uses the same managed-copy, sidecar,
metadata, artwork-cache, file-hash, and audio-hash deduplication behavior as the app.

## Music views and tracks

A track selector can be an exact managed path, `view_id`, or track id.

```bash
silent --cli --db ./silent.sqlite3 --media-root ./Music track show <selector>
silent --cli --db ./silent.sqlite3 --media-root ./Music track edit <selector> --name "Phone edit" --title "Title" --notes "Note"
silent --cli --db ./silent.sqlite3 --media-root ./Music track metadata set <selector> --title "Title" --artist "Artist" --album "Album"
silent --cli --db ./silent.sqlite3 --media-root ./Music track notes set <selector> "Notes"
silent --cli --db ./silent.sqlite3 --media-root ./Music track rate <selector> 8
silent --cli --db ./silent.sqlite3 --media-root ./Music track rate <selector> clear
silent --cli --db ./silent.sqlite3 --media-root ./Music track artwork set <selector> ./cover.png
silent --cli --db ./silent.sqlite3 --media-root ./Music track album-artwork set <selector> ./album-cover.jpg
silent --cli --db ./silent.sqlite3 --media-root ./Music track lyrics set <selector> ./song.lrc
silent --cli --db ./silent.sqlite3 --media-root ./Music track export <selector> ./Portable.flac
silent --cli track analyze ./song.flac
```

Metadata, notes, track artwork, lyrics, and combined `track edit` operations create derived music
views according to the same rules as the app. Export/materialize creates a new independent primary
view.

## Favorites, playlists, history, and user data

```bash
silent --cli --db ./silent.sqlite3 --media-root ./Music favorites list
silent --cli --db ./silent.sqlite3 --media-root ./Music favorites add <selector>
silent --cli --db ./silent.sqlite3 --media-root ./Music favorites remove <selector>

silent --cli --db ./silent.sqlite3 --media-root ./Music playlist list
silent --cli --db ./silent.sqlite3 --media-root ./Music playlist create "Road Trip"
silent --cli --db ./silent.sqlite3 --media-root ./Music playlist add "Road Trip" <selector>
silent --cli --db ./silent.sqlite3 --media-root ./Music playlist move "Road Trip" <selector> up
silent --cli --db ./silent.sqlite3 --media-root ./Music playlist sort "Road Trip" album
silent --cli --db ./silent.sqlite3 --media-root ./Music playlist artwork set "Road Trip" ./cover.png
silent --cli --db ./silent.sqlite3 --media-root ./Music playlist rename "Road Trip" "Highway"
silent --cli --db ./silent.sqlite3 --media-root ./Music --yes playlist clear "Highway"
silent --cli --db ./silent.sqlite3 --media-root ./Music --yes playlist delete "Highway"

silent --cli --db ./silent.sqlite3 --media-root ./Music history list --limit 100
silent --cli --db ./silent.sqlite3 --media-root ./Music user show
```

## Playback

Playback controls live in one process so the audio engine and playback history session remain
alive:

```bash
silent --cli --db ./silent.sqlite3 --media-root ./Music playback shell
silent --cli --db ./silent.sqlite3 --media-root ./Music playback shell <selector> <selector> ...
```

Inside the shell:

```text
play <selector>
load <selector>...
pause
resume
stop
next
previous
seek 1:23
status
queue
repeat off|one|all
shuffle on|off
lifecycle interruption-begin
lifecycle interruption-end on|off
lifecycle output-disconnected
quit
```

## Target parity

| Shared capability | macOS | iPhone | CLI |
| --- | --- | --- | --- |
| Managed file/folder import | Yes | Yes | Yes |
| Library package export/import/zero | Yes | Yes | Yes |
| Search, loudness analysis, database audit | Yes | Yes | Yes |
| Music View details/edit/rating/artwork/lyrics/export | Yes | Yes | Yes |
| Favorites, playlists, history, local user data | Yes | Yes | Yes |
| Queue, seek, repeat, shuffle, lifecycle rules | Yes | Yes | Yes, in playback shell |
| Lock screen, headset commands, Now Playing, background audio session | Yes | Yes | Not applicable |

The internal `player_analyzer` and `player_library_worker` executables remain app workers for
progress reporting and cancellation. They are not public CLI entry points.
