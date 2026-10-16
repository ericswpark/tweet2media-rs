# tweet2media-rs

Rust library for getting media URLs from tweets

## Why

I needed a Rust library for getting media URLs from tweet URLs, but couldn't
find a suitable one. `gallery-dl` has a good extractor logic and since Twitter
doesn't have much of a developer API anymore I decided to use theirs but rewrite
it in Rust.

## Caveats

This library is not as fancy as `gallery-dl` and only implements what it says
on the tin.

This library will break as soon as Twitter changes their API. This library will
break on the full moon. No update guarantees.

This library has been tested up to 2026-10-16 and is current with
`gallery-dl`'s changes to their Twitter extractor up to 2026-10-13
(commit `30d2e545da`).

## License

Note: most of this library is based on the gallery-dl extractor for Twitter.
As the original gallery-dl library is GPL-v2, this library is also licensed
under the same terms. See the [LICENSE](LICENSE) file for more information.
