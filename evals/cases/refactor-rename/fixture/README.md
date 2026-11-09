# acme users

`fetchUsr(id)` in `api.ts` returns the user with that id, or `undefined`.
`profile.ts`, `admin.ts` and `cache.ts` build on `fetchUsr`.
