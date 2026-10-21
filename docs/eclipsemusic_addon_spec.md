# Eclipse Music Addon Specification & Implementation Guide

Reference documentation extracted from https://eclipsemusic.app/docs for building Eclipse Music Addon support.

---

## 1. Overview & Architecture

Eclipse Music (`https://eclipsemusic.app`) allows custom HTTP addons for music discovery, streaming, and catalog browsing.
An addon is an HTTP server returning JSON with CORS headers (`Access-Control-Allow-Origin: *`).

### Core Endpoints Summary

| Endpoint                                     | Method | Required         | Purpose                                   |
| :------------------------------------------- | :----- | :--------------- | :---------------------------------------- |
| `/manifest.json` or `/eclipse/manifest.json` | `GET`  | **Yes**          | Returns addon metadata & capabilities     |
| `/search?q={query}`                          | `GET`  | **Yes**          | Search tracks, albums, artists, playlists |
| `/stream/{id}`                               | `GET`  | **Yes**          | Stream URL resolver                       |
| `/resolve-isrc?isrc={isrc}`                  | `GET`  | No (Recommended) | Exact ISRC match lookup                   |
| `/resolve?isrc=...&title=...&artist=...`     | `GET`  | No (Recommended) | Resolve recording for Radio/Mixes/AI DJ   |
| `/album/{id}`                                | `GET`  | No               | Album tracks & details                    |
| `/artist/{id}`                               | `GET`  | No               | Artist top tracks & albums                |
| `/playlist/{id}`                             | `GET`  | No               | Playlist details & tracklist              |
| `/catalog/{id}?skip=0`                       | `GET`  | No               | Browsable home/discover catalog rows      |

---

## 2. Manifest (`GET /manifest.json`)

Describes the addon's identifier, display name, supported resources, and settings.

### Schema

```json
{
  "id": "org.pencarimovie.eclipsemusic",
  "name": "PencariMusic",
  "version": "1.0.0",
  "description": "Stream Telegram audio & music files via PencariMovie",
  "icon": "https://example.com/icon.png",
  "resources": ["search", "stream", "resolve", "catalog", "settings"],
  "types": ["track", "album", "artist", "playlist"],
  "contentType": "music",
  "catalogs": [
    { "id": "latest", "type": "track", "name": "Latest Music" },
    { "id": "albums", "type": "album", "name": "Albums & Collections" }
  ],
  "settings": [
    {
      "key": "quality",
      "type": "select",
      "label": "Audio quality",
      "default": "high",
      "options": [
        { "value": "high", "label": "High (320kbps / FLAC)" },
        { "value": "normal", "label": "Normal (160kbps)" }
      ]
    }
  ]
}
```

### Manifest Fields

- `id` (string, required): Unique identifier (e.g. `org.pencarimovie.eclipsemusic`).
- `name` (string, required): Display name shown in search & player.
- `version` (string, required): Semver string (`1.0.0`).
- `resources` (array of string, required): Available capabilities:
  - `"search"`
  - `"stream"`
  - `"catalog"` (enables `/catalog/{id}` as well as `/album/{id}`, `/artist/{id}`, `/playlist/{id}`)
  - `"isrc"` (enables `/resolve-isrc?isrc=`)
  - `"resolve"` (enables `/resolve?isrc=&title=&artist=`)
  - `"settings"` (enables user settings UI)
- `types` (array of string, optional): `["track", "album", "artist", "playlist"]`.
- `contentType` (string, optional): `"music"` (default), `"audiobook"`, or `"podcast"`.
- `icon` (string, optional): Square PNG/JPG icon URL (128x128+).
- `catalogs` (array of object, optional): Custom rows for Home/Discover screens.
- `settings` (array of object, optional): User configuration schema.

---

## 3. Search (`GET /search?q={query}`)

Called when searching in Eclipse or during playlist matching.

### Response Schema

```json
{
  "tracks": [
    {
      "id": "track_123",
      "title": "Song Title",
      "artist": "Artist Name",
      "album": "Album Name",
      "duration": 240,
      "artworkURL": "https://example.com/cover.jpg",
      "isrc": "USRC12345678",
      "format": "mp3",
      "streamURL": "https://example.com/audio.mp3"
    }
  ],
  "albums": [
    {
      "id": "album_456",
      "title": "Album Title",
      "artist": "Artist Name",
      "artworkURL": "https://example.com/album.jpg",
      "trackCount": 12,
      "year": "2024"
    }
  ],
  "artists": [
    {
      "id": "artist_789",
      "name": "Artist Name",
      "artworkURL": "https://example.com/artist.jpg",
      "genres": ["Pop", "Rock"]
    }
  ],
  "playlists": [
    {
      "id": "playlist_123",
      "title": "Best of 2024",
      "creator": "Curator Name",
      "artworkURL": "https://example.com/playlist.jpg",
      "trackCount": 50
    }
  ]
}
```

- If `streamURL` is included on a track, Eclipse can play directly without calling `/stream/{id}`.
- If `isrc` is present, Eclipse enriches metadata using Apple Music / MusicKit.
- All 4 arrays are optional.

---

## 4. Stream (`GET /stream/{id}`)

Called when a user taps play on a track without a static `streamURL`.

### Response Schema

```json
{
  "url": "https://server.domain/api/download/BASE64/track.mp3",
  "format": "mp3",
  "quality": "320kbps",
  "expiresAt": 1726315200,
  "codec": "mp3",
  "container": "mp3",
  "manifest": "none",
  "sampleRate": 44100,
  "bitDepth": 16,
  "video": {
    "url": "https://server.domain/api/download/BASE64/video.mp4",
    "mimeType": "video/mp4",
    "muxed": true,
    "width": 1920,
    "height": 1080
  }
}
```

### Fields

- `url` (string, required): Direct audio HTTP/HTTPS link.
- `format` (string, optional): `"mp3"`, `"flac"`, `"aac"`, `"m4a"`, `"wav"`, `"ogg"`.
- `quality` (string, optional): Display string, e.g. `"320kbps"`, `"lossless"`.
- `expiresAt` (number, optional): Unix epoch timestamp.
- `codec` (string, optional): `flac`, `aac`, `he-aac`, `alac`, `mp3`, `opus`, `vorbis`.
- `container` (string, optional): `flac`, `mp3`, `mp4`, `fmp4`, `ogg`, `wav`.
- `manifest` (string, optional): `none`, `hls`, `dash`.
- `sampleRate` (number, optional): e.g. `44100`, `48000`, `96000`.
- `bitDepth` (number, optional): `16`, `24`.
- `video` (object, optional): If music video exists:
  - `url`: video URL.
  - `mimeType`: e.g. `"video/mp4"`.
  - `muxed`: `true` (audio+video combined).

---

## 5. Detail Endpoints (Optional / Catalog)

### Album Details (`GET /album/{id}`)

```json
{
  "id": "album_456",
  "title": "Album Title",
  "artist": "Artist Name",
  "artworkURL": "https://example.com/album.jpg",
  "year": "2024",
  "description": "Album description",
  "trackCount": 10,
  "tracks": [
    {
      "id": "track_1",
      "title": "Track 1",
      "artist": "Artist Name",
      "duration": 210,
      "streamURL": "https://example.com/track1.mp3"
    }
  ]
}
```

### Artist Details (`GET /artist/{id}`)

```json
{
  "id": "artist_789",
  "name": "Artist Name",
  "artworkURL": "https://example.com/artist.jpg",
  "bio": "Bio",
  "genres": ["Pop"],
  "topTracks": [...],
  "albums": [...]
}
```

### Playlist Details (`GET /playlist/{id}`)

```json
{
  "id": "playlist_123",
  "title": "Playlist Name",
  "description": "Description",
  "artworkURL": "https://example.com/cover.jpg",
  "creator": "Curator",
  "tracks": [...]
}
```

---

## 6. Exact Resolution (`/resolve` & `/resolve-isrc`)

Used by Eclipse for Radio, AI DJ, Autoplay, and Smart Shuffle.

### `GET /resolve-isrc?isrc={isrc}`

Return exact track ID or `404` / `{"trackId": null}`:

```json
{
  "trackId": "track_123"
}
```

### `GET /resolve?isrc={isrc}&title={title}&artist={artist}&durationMs={ms}`

Strict match lookup:

```json
{
  "item": {
    "id": "track_123",
    "type": "track",
    "title": "Spend Dat",
    "artist": "Yung Miami",
    "isrc": "USUG12400123"
  }
}
```

If not found, respond with HTTP 200:

```json
{
  "item": null
}
```

---

## 7. Catalogs (`GET /catalog/{id}?skip=0`)

Returns items for declared rows in manifest `catalogs`:

```json
{
  "items": [
    {
      "id": "track_123",
      "type": "track",
      "title": "Song Title",
      "artist": "Artist Name",
      "album": "Album Name",
      "durationMs": 180000,
      "isrc": "USRC12345678",
      "artworkURL": "https://example.com/cover.jpg"
    }
  ]
}
```

`skip` is in increments of 100. Fewer than 100 items returned signals the end of the row.

---

## 8. Settings Integration

Declared in manifest `settings`. Eclipse appends current values as query parameters to **all** requests:

```http
GET /stream/track_123?quality=high&region=US
```

Values can be read from `$_GET['quality']` etc.

---

## 9. Requirements for PencariMovie Server

1. **CORS Headers**: `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, OPTIONS`, `Access-Control-Allow-Headers: *`.
2. **Path Support**:
   - `/eclipse/manifest.json` (or `/manifest.json` with mode / audio handling)
   - `/eclipse/search?q=...`
   - `/eclipse/stream/{id}`
   - `/eclipse/resolve-isrc?isrc=...`
   - `/eclipse/resolve?...`
   - `/eclipse/catalog/{id}?skip=0`
3. Direct audio streaming links pointing to PencariMovie's MadelineProto streaming endpoint (`/api/download/{base64}/{filename}.mp3` or `.flac`).
