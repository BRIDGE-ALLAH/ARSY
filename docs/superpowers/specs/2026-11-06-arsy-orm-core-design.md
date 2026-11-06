# Spesifikasi SP1 — ARSY ORM Core + Base Add-on

- Status: menunggu review
- Tanggal: 2026-11-06
- Induk: `docs/ARCHITECTURE.md` (roadmap SP1)

## 1. Tujuan & non-tujuan

**Tujuan (definition of done SP1):**
1. Crate `arsy-orm` + `arsy-orm-macros` mampu mendeklarasikan model gaya Odoo
   dengan `#[derive(Model)]`, membuat/meng-evolusi tabel Postgres otomatis,
   dan melakukan CRUD + search domain minimal melalui `sqlx`.
2. `ActorContext`, automatic columns, `audit_log`, dan pipeline default data
   sudah aktif sejak model pertama — bukan tempelan kemudian.
3. `addons/base` menjadi rumah model `res.User` dkk; `arsy-server` yang ada
   sekarang bermigrasi dari `UserRepository` in-memory ke ORM Postgres,
   dengan test integrasi hijau.

**Non-tujuan (explicitly out of scope SP1):**
- RLS **OFF** (kolom `tenant_id` + konstanta policy sudah digenerate tapi
  `ENABLE ROW LEVEL SECURITY` belum; dinyalakan SP2).
- Auth JWT/API-key (middleware auth = stub yang mengembalikan `System` actor).
- Relasi Many2one/O2o/M2M, computed fields, `_inherit`, ACL, router
  generatif `/rest/*`, storage, console, CLI.

## 2. Crate & dependensi baru

```
crates/arsy-orm/          lib: model, fields, domain, ddl, module, registry,
                          actor, uow, audit, data
crates/arsy-orm-macros/   proc-macro: derive Model + atribut #[orm(...)]
addons/base/              lib: model res.* + manifest + data/*.toml
```

Workspace dependencies baru: `sqlx = { version = "0.8", features =
["runtime-tokio", "postgres", "uuid", "chrono", "macros" ] }` — macros DINONAKTIFKAN
(query runtime string, supaya snapshot SQL bisa dites; `sqlx::query!` butuh DB
saat compile dan tidak cocok dengan flow kita), `linkme = "0.3"`,
`uuid = { features = ["v7"] }`, `chrono = { features = ["serde"] }`,
`toml = "0.9"`. Dev: `testcontainers` +
`testcontainers-modules = { postgres }`.

`arsy-core::services::UserRepository` tetap ada selama transisi lalu DELETED
saat migrasi selesai (lihat §9).

## 3. API model — bentuk final v1

```rust
#[derive(Model)]
#[orm(addon = "base", name = "res.users", table = "res_users")]
pub struct ResUsers {
    #[orm(id)]
    pub id: Uuid,                          // UUIDv7, digenerate ORM saat create
    #[orm(field(char, required))]
    pub name: String,
    #[orm(field(char, required, unique))]
    pub login: String,
    #[orm(field(char, audit = false))]
    pub password_hash: Option<String>,
    #[orm(field(boolean, default = true))]
    pub active: bool,
    #[orm(field(timestamptz))]
    pub last_login: Option<DateTime<Utc>>,
    // kolom otomatis — TIDAK dideklarasikan user, tersedia via aksesors:
    // tenant_id: Uuid (Nilai default = tenant actor)
    // created_uid/at, write_uid/at
}
```

Aturan derive:
- struct field wajib `pub` + `#[orm(...)]`; field tanpa atribut = compile error.
- tipe field v1 ↔ PG: `char|text→TEXT`, `integer→BIGINT`, `float→DOUBLE
  PRECISION`, `boolean→BOOLEAN`, `date→DATE`, `timestamptz→TIMESTAMPTZ`,
  `uuid→UUID`, `jsonb→JSONB`. `Option<T>` = NULL, `T` = NOT NULL.
- properti field: `required`, `default = <literal|fn>`, `unique`,
  `index`, `selection = [..]` (validasi + CHECK di DB), `audit = false`.
- `#[orm(audit = false)]` pada field → delta audit tidak menyimpan nilainya
  (untuk password_hash sejak awal).

Method bawaan per model (semua async, semua menerima `&Uow` + actor):

```rust
ResUsers::create(uow, actor, values)   -> Id<ResUsers>
ResUsers::browse(uow, actor, id)       -> Record<ResUsers>     // read-cache
ResUsers::write(uow, actor, ids, values) -> ()
ResUsers::unlink(uow, actor, ids)      -> ()
ResUsers::search(uow, actor, domain, limit, offset) -> Vec<Id<ResUsers>>
ResUsers::search_count(uow, actor, domain) -> i64
```

`Record<T>` lazy-load field dari registry + prefetch cache read-only (§7).

## 4. ActorContext & Unit of Work

```rust
pub struct ActorContext {
    pub tenant: TenantId,
    pub actor: Actor,          // System | User(Uuid) | (ApiKey/Service SP2+)
    pub roles: BTreeSet<String>,
}
pub struct Uow { tx: Mutex<sqlx::Transaction<'static, Postgres>> }
```

- `Uow::begin(pool, actor)` → `BEGIN; SET LOCAL app.tenant_id = $1` — statement
  ini SELALU yang pertama; nilai di-cast `uuid` di sisi Rust (bukan string
  concat).
- Request handler membikin satu `Uow`; `arsy-server` saat ini merakitnya di
  layer route (pola yang sama dipakai router generatif SP3).
- `uow.commit()` mengakhiri; drop tanpa commit = rollback.
- Actor `System` = satu-satunya yang boleh `tenant = TenantId::PLATFORM`
  (untuk bootstrap `ir_module`, provisioning nanti).

## 5. Audit trail

Tabel (dimiliki add-on `base`):

```sql
CREATE TABLE audit_log (
  id uuid PRIMARY KEY, tenant_id uuid NOT NULL,
  model text NOT NULL, res_id uuid NOT NULL,
  action text NOT NULL CHECK (action IN ('create','write','unlink')),
  delta jsonb NOT NULL,           -- {"field": {"from": ..., "to": ...}}
  actor jsonb NOT NULL,           -- {"type":"user","id":"..."} | {"type":"system"}
  at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_scope ON audit_log (tenant_id, model, res_id, at DESC);
```

- Ditulis DI DALAM Uow yang sama dengan operasi model (invariant #4).
- Model opt-in lewat `#[orm(track = "audit")]` pada struct; tanpa itu tidak
  ada jejak. Field `audit = false` di-scrub dari delta.
- create: delta = seluruh nilai; unlink: delta = snapshot sebelum hapus.

## 6. Domain compiler v1

```rust
let users = ResUsers::search(uow, &actor,
    domain!(("login", ilike, "%ada%"), "&", ("active", =, true)), 10, 0)?;
```

- AST node: leaf `{field, op, value}` + polish operator `&`/`|`/`!` (bentuk
  Odoo). v1 operator: `= != in ilike like > >= < <=`.
- `domain!` macro → `Domain` (bukan macro expansion SQL); compiler
  `Domain::to_sql(&ModelMeta) -> (String /*$i placeholders*/, Vec<DbValue>)`.
- Nama field divalidasi terhadap metadata saat **build query** (field tak
  dikenal = error, bukan SQL mentah); identifier tabel/kolom di-quote; nilai
  selalu bind. Snapshot test wajib memuat kasus injection-ish values.
- ORDER: v1 default `id DESC` (UUIDv7 ≈ kronologis); `order_by` eksplisit
  menyusul.

## 7. Read cache (registry)

- `DataRegistry` per-Uow: `model → id → Record-values` hasil read terakhir;
  `search` mengembalikan id, `browse` massal prefetch dalam 1 query `IN`.
- Write selalu flush ke DB lalu invalidate entri cache. Tidak ada write-buffer
  (keputusan #6 arsitektur).

## 8. ModuleRegistry, DDL, dan default data

### 8.1 Manifest & registrasi
- Setiap crate add-on mendeklarasikan model via slice `linkme`
  `MODEL_REGISTRY: [ModelMeta]` yang diisi derive macro; `ModelMeta` memuat
  `addon`, `name`, `table`, `fields`, `owner_addon`.
- Add-on itu sendiri terdaftar via `[package.metadata.arsy.addon]` +
  link-time `ADDON_MANIFESTS`; `ModuleRegistry::from_crate()` membacanya saat
  start, cek: semua `depends` ada dan ter-resolve topologis, jika tidak →
  gagal start dengan pesan eksplisit.

### 8.2 Tabel kendali
```sql
CREATE TABLE ir_module (
  name text PRIMARY KEY, version text NOT NULL,
  state text NOT NULL CHECK (state IN ('uninstalled','to install','installed','to remove')),
  latest_version text NOT NULL, dependencies jsonb NOT NULL,
  installed_at timestamptz
);
CREATE TABLE ir_model_data (           -- external id, seperti Odoo
  module text NOT NULL, name text NOT NULL, model text NOT NULL,
  res_id uuid NOT NULL, tenant_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
  noupdate bool NOT NULL DEFAULT false,
  PRIMARY KEY (tenant_id, module, name)
);
```

### 8.3 Pipeline install (satu Uow System per add-on)
1. toposort depends → 2. jalankan `migrations/<ver>/*.sql` yang tertinggal →
3. DDL sync: untuk tiap model milik add-on: `CREATE TABLE IF NOT EXISTS`;
   kolom baru di-`ADD COLUMN` (nullable/defaults); kolom hilang **tidak**
   pernah di-drop otomatis (v1: diwarnai di log); tipe berubah → error
   eksplisit minta migrasi manual → 4. muat `data/*.toml` → 5. set
   `ir_module.state='installed'`.

### 8.4 Format default data (toml, upsert external-id)
```toml
[[record]]
xml_id = "base.user_admin"
model = "res.users"
values = { name = "Administrator", login = "admin", active = true }
noupdate = false
```
Upsert via `INSERT ... ON CONFLICT` pada `(tenant_id, module, name)`;
`noupdate = true` → update hanya bila baris belum ada. Referensi
`@base.company_root` di dalam values → lookup xml_id (two-pass: record yang
mereferensi dimuat setelah yang direferensi).

## 9. Migrasi aplikasi yang ada

- `addons/base` mendeklarasikan `ResUsers` + data default (`user_admin`).
- `arsy-server`: `AppState` menyimpan `PgPool`; route v1 membuat `Uow` per
  request; handler user memanggil `ResUsers::*` alih-alih
  `Arc<UserService>`; `ApiError::from` menangani `OrmError`
  (`NotFound→404`, `Validation→400`, `Db/Internal→500` + log).
- Hapus `arsy_core::services::UserRepository`, `UserService`,
  `arsy_infra::db::InMemoryUserRepository` (modul `clients/` tetap untuk
  fase depan), dan test lama mereka; `tests/api.rs` ditulis ulang melawan
  Postgres nyata (testcontainers).
- `deployments/docker-compose.yml`: tambah service `postgres:17-alpine` +
  healthcheck + volume; `DATABASE_URL` ke `.env.example`.

## 10. Error model

```rust
pub enum OrmError { NotFound(String), Validation(String), Constraint(String),
                    Domain(String), Module(String), Db(sqlx::Error) }
```
`Display` aman untuk client (tanpa SQL detail); `Db` hanya muncul sebagai
`Internal` di lapisan API (pola `errors.rs` yang sudah ada).

## 11. Rencana testing (gate penerimaan)

1. **Unit murni**: parser domain + toposort manifest + resolusi xml_id.
2. **Snapshot SQL**: DDL per model, WHERE hasil compile (kasus: nilai
   `"; DROP` , tipe value salah, field tak dikenal).
3. **Integrasi (testcontainers Postgres)**: install `base` idempoten (2x
   start = tidak duplikat); CRUD roundtrip incl. automatic columns terisi;
   audit delta benar & scrub field; create duplicate `login` → Constraint;
   upsert default data respect `noupdate`; xml_id reference resolve.
4. **Aplikasi**: `tests/api.rs` versi ORM hijau terhadap Postgres container;
   healthz tidak berubah.
5. CI: job `postgres` service ditambahkan ke `ci.yml`; `just test` lokal
   butuh docker berjalan (didokumentasikan di README).

## 12. Risiko & mitigasi

| Risiko | Mitigasi |
|---|---|
| Derive macro jadi rawan bug | derive setip tipis: hasilkan `impl Model` dari metadata statik; logika berat di `arsy-orm` yang teruji unit |
| `SET LOCAL` bocor antar request | hanya dalam transaksi + test integrasi dua Uow paralel beda tenant (RLS off, cek `current_setting`) |
| uuid vs bigint di Odoo-parity | diputus: UUIDv7; kolom `serial` internal TIDAK ada |
| Drop-kolom tak sengaja saat refactor model | v1 menolak auto-drop; drift dilaporkan `arsy orm diff` (perintah CLI di SP3, v1: fungsi lib + test) |
| Scope SP1 melebar | fitur apa pun yang bukan §1 Tujuan ditolak ke SP2+ |
