# 0496 WebView2's account sign-on peer is listed, and looked at again after v1

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
After the campaign's WebView2 switches, the companion window's WebView2 still opens one connection appendix I doesn't list, to Microsoft's Outlook/Substrate service. List it, chase it before v1, or accept it?

## Facts
Verified 2026-10-24 in the campaign's egress checks (0481).
- `--disable-background-networking --disable-component-update --disable-features=msSmartScreenProtection` (`src/app/webview.ts`) removed `edge.microsoft.com`, the port-80 download CDN and SmartScreen.
- A 19-minute NetLog of WebView2's network stack shows only loopback (the page and whiteboard), failed `wpad` lookups and Chromium's IPv6 probe.
- WebView2's browser process opens two sockets to an address of `outlook.office365.com` / `substrate.office.com` (CNAME `outlook.cloud.microsoft`; 52.97.148.210, 40.104.205.210) 7 s after start, outside its network stack. The process has `oneauth.dll` and Windows' account-broker extensions loaded, and the profile holds `msa_sso_info` and `profile_sso_info`. That points at Edge's account single sign-on; proving the host name needs an admin packet capture.
- Electrobun 2.0.2's native WebView2 setup (Hutch 0.27.1) is downloaded at build time; its source isn't in the repo.

## Options
| Option | Pros | Cons |
|---|---|---|
| List it, look again after v1 | The egress list is complete for v1; a known follow-up | The connection stays until then |
| Chase it before v1 | It may go away | Electrobun's native source isn't here; a fix may need a patched fork (0383) |
| Just list it | Done | No follow-up |

## Recommendation
List it, look again after v1.

## Choice
Plan appendix I lists it: the companion window's WebView2 runtime contacts Microsoft's Outlook/Substrate service (`outlook.office365.com` / `substrate.office.com`), most likely its own account single sign-on, not anything the page or Jocasta sends. After v1, check whether Electrobun's WebView2 setup turns single sign-on on, and switch it off there.

## Reason
"List it, look again after v1".

## Consequences
- `docs/status.md` carries the follow-up among the after-v1 items.
- Amended by 0514: Electrobun 2.0.2 never turns WebView2's sign-on on (its default is off), so nothing is switched off there; the follow-up is an egress check of a bare WebView2 host after v1.
