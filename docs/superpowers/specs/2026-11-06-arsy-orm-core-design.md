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
- Relasi penuh (Many2one record/prefetch — SP1 hanya kolom FK polos, §3),
  computed fields, `_inherit`, ACL, router generatif `/rest/*`, storage,
  `ir.Attachment` (lahir di SP4 bersama storage), console, CLI.

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
- `unique` SELALU dikompilasi menjadi `UNIQUE (tenant_id, col)` — tidak
  pernah global, agar tenant lain boleh memakai login yang sama dan error
  duplikat tidak membocorkan data tenant lain. `unique = global`
  eksplisit hanya untuk tabel non-tenant (mis. `ir_module`).
- `#[orm(audit = false)]` pada field → delta audit tidak menyimpan nilainya
  (untuk password_hash sejak awal).
- Relasi v1 = **FK polos**: `#[orm(field(reference = "res.company"))]` →
  kolom uuid + `REFERENCES res_company(id)`; search via nilai FK sudah
  mungkin; lazy record/prefetch menyusul (SP6). Tanpa ini domain `base`
  sendiri tidak bisa diekspresikan.

Method bawaan per model (semua async, semua menerima `&Uow` + actor):

```rust
ResUsers::create(uow, actor, values)      -> Id<ResUsers>
ResUsers::browse(uow, actor, id)          -> Record<ResUsers>   // read-cache
ResUsers::write(uow, actor, ids, values)  -> ()   // kunci baris FOR UPDATE (§4)
ResUsers::unlink(uow, actor, ids)         -> ()   // HARD delete, jalur purge/GDPR
ResUsers::toggle_active(uow, actor, ids)  -> ()   // archive/unarchive, bukan unlink
ResUsers::search(uow, actor, domain, limit, offset) -> Vec<Id<ResUsers>>
ResUsers::search_count(uow, actor, domain) -> i64
```

`Record<T>` lazy-load field dari registry + prefetch cache read-only (§7).

### 3.1 Model::validate — rumah aturan bisnis

Sebelum persist (create & write) ORM memanggil hook pada record gabungan
hasil, setelah pengecekan level field (required/selection/tipe):

```rust
impl ResUsers {
    pub fn validate(&self, uow: &Uow, actor: &ActorContext,
                    changed: &[FieldName]) -> Result<(), OrmError> { /* ... */ }
}
```

- Rumah aturan lintas-field: normalisasi trim/lowercase, format email,
  cek duplikat via `search` — analog `@api.constrains` Odoo.
- Keputusan review: aturan lama `UserService` pindah ke sini 1:1 (§9).
- Default implement disediakan derive macro (no-op) agar model tanpa
  aturan tetap kompilasi; aturannya tidak boleh tersebar di handler.
- Semantik archive: `toggle_active()` ≠ `unlink()`; search default
  mengecualikan yang archived (`active_test`, §6).

## 4. ActorContext & Unit of Work

```rust
pub struct ActorContext {
    pub tenant: TenantId,
    pub actor: Actor,          // System | User(Uuid) | (ApiKey/Service SP2+)
    pub roles: BTreeSet<String>,
}
pub struct Uow { conn: Mutex<sqlx::PgConnection> }  // BEGIN + SET LOCAL saat creation
```

- `Uow::begin(pool, actor)` → `BEGIN; SET LOCAL app.tenant_id = $1` — statement
  ini SELALU yang pertama; nilai di-cast `uuid` di sisi Rust (bukan string
  concat).
- Request handler membikin satu `Uow`; `arsy-server` saat ini merakitnya di
  layer route (pola yang sama dipakai router generatif SP3).
- `uow.commit()` mengakhiri; drop tanpa commit = rollback.
- **Konkurensi v1 = pessimistic** (keputusan): `write()`/`unlink()`
  mengunci baris target dengan `SELECT ... FOR UPDATE` di dalam Uow sebelum
  UPDATE/DELETE; deadlock → `OrmError::Db` → lapisan API memetakan ke 409
  dengan pedoman retry ke client, tanpa retry senyap di ORM. Lost update
  tidak bisa terjadi by default.
- `Uow` di-scope satu task (Mutex menjaga akses koneksi dari helper, bukan
  untuk sharing antar task); signature publik tetap `&Uow`.
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
  selalu bind; operator `in` di-expand per chunk ≤ 500 placeholder (tetap
  bind). Snapshot test wajib memuat kasus injection-ish values.
- **`active_test`** (konvensi Odoo): `search`/`search_count` pada model
  ber-field `active` otomatis di-conjoin `("active", =, true)` kecuali uow
  memilih `with_inactive`.
- ORDER: v1 default `id DESC` (UUIDv7 ≈ kronologis); `order_by` eksplisit
  menyusul. `ilike '%…'` butuh trigram index — dicatat untuk SP+.

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
- Metadata `[package.metadata.arsy.addon]` di Cargo.toml **tidak terbaca di
  runtime** — karena itu `build.rs` tiap add-on membaca Cargo.toml-nya saat
  compile dan meng-embed manifest sebagai `#[used] static ADDON_MANIFEST`
  (terkumpul di slice `ADDON_MANIFESTS`). `ModuleRegistry::assemble()`
  membangun graf saat start: semua `depends` ada dan ter-topologis, jika
  tidak → gagal start dengan pesan eksplisit.

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
1. toposort depends → 2. **`pg_advisory_xact_lock(hashtext(module))`** —
   dua instance server yang start bersamaan tidak boleh install modul yang
   sama → 3. **baseline**: bila baris `ir_module` baru (install pertama),
   folder `migrations/<ver>/*.sql` DILEWATI dan `latest_version` diisi
   setelah sync; bila upgrade, jalankan migrasi yang tertinggal → 4. DDL
   sync: untuk tiap model milik add-on: `CREATE TABLE IF NOT EXISTS`;
   kolom baru di-`ADD COLUMN` (nullable atau dengan default eksplisit —
   NOT NULL tanpa default pada tabel terisi = error); kolom hilang **tidak**
   pernah di-drop otomatis (v1: drift dilaporkan di log); tipe berubah →
   error eksplisit minta migrasi manual → 5. muat `data/*.toml` → 6. set
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
- **Kriteria paritas validasi**: aturan lama UserService (trim name, format
  email + lowercase, duplikat case-insensitive, envelope 400) harus lolos
  1:1 di `tests/api.rs` baru — logikanya tinggal di `ResUsers::validate`,
  `UNIQUE (tenant_id, login)` di DB sebagai backstop.

## 10. Error model

```rust
pub enum OrmError {
    NotFound(String),
    Validation(String),  // field-level & Model::validate
    Constraint { field: String, message: String }, // DB unique/CHECK/FK
    Domain(String),
    Module(String),
    Db(sqlx::Error),     // termasuk deadlock/serialization
}
```
`Constraint` selalu menyebut field pelanggarnya (sumber pesan 400/409 yang
berguna); `Display` aman untuk client (tanpa SQL detail); `Db` menjadi
`Internal` di lapisan API (pola `errors.rs` yang sudah ada), kecuali
deadlock yang dipetakan ke 409 dengan pedoman retry (keputusan §4).

## 11. Rencana testing (gate penerimaan)

1. **Unit murni**: parser domain + toposort manifest + resolusi xml_id.
2. **Snapshot SQL**: DDL per model, WHERE hasil compile (kasus: nilai
   `"; DROP` , tipe value salah, field tak dikenal).
3. **Integrasi (testcontainers Postgres)**: install `base` idempoten (2x
   start = tidak duplikat); **dua install paralel = satu hasil bersih**
   (advisory lock, tanpa deadlock/duplikat data); CRUD roundtrip incl.
   automatic columns terisi; audit delta benar & scrub field; create
   duplicate `login` → Constraint; **login sama antar beda tenant = OK**
   (unique tenant-scoped); upsert default data respect `noupdate`; xml_id
   reference resolve.
4. **Perilaku (regression dari deep review)**: paritas `validate` (email
   invalid → 400, duplikat → pesan menyebut field); `toggle_active` +
   `active_test` (archived tidak muncul di search default); konkurensi:
   dua `write()` paralel satu baris ter-serialisasi tanpa lost update.
5. **Aplikasi**: `tests/api.rs` versi ORM hijau terhadap Postgres container;
   healthz tidak berubah.
6. CI: job `postgres` service ditambahkan ke `ci.yml`; `just test` lokal
   butuh docker berjalan + resep `just db` (`docker compose -f
   deployments/docker-compose.yml up -d postgres`); README diperbarui.

## 12. Risiko & mitigasi

| Risiko | Mitigasi |
|---|---|
| Derive macro jadi rawan bug | derive setip tipis: hasilkan `impl Model` dari metadata statik; logika berat di `arsy-orm` yang teruji unit |
| `SET LOCAL` bocor antar request | hanya dalam transaksi + test integrasi dua Uow paralel beda tenant (RLS off, cek `current_setting`) |
| uuid vs bigint di Odoo-parity | diputus: UUIDv7; kolom `serial` internal TIDAK ada |
| Drop-kolom tak sengaja saat refactor model | v1 menolak auto-drop; drift dilaporkan `arsy orm diff` (perintah CLI di SP3, v1: fungsi lib + test) |
| Dua instance start bersamaan → double install | `pg_advisory_xact_lock` per module (§8.3) |
| Unique global membocorkan / tabrakan antar tenant | `unique` dikompilasi `(tenant_id, col)` by default (§3) |
| Lost update tanpa kontrol konkurensi | v1 pessimistic `FOR UPDATE` (§4); deadlock eksplisit 409 |
| Aturan bisnis hilang saat migrasi dari UserService | paritas diuji di §11.4; rumah aturan = `Model::validate` (§3.1) |
| Scope SP1 melebar | fitur apa pun yang bukan §1 Tujuan ditolak ke SP2+ |

## Lampiran A — Bentuk `ModelMeta` (kontrak derive ↔ orm)

```rust
pub struct ModelMeta {
    pub addon: &'static str,           // "base"
    pub name: &'static str,            // "res.users"
    pub table: &'static str,           // "res_users"
    pub fields: &'static [FieldMeta],
    pub audit: AuditMode,              // Off | Tracked
    pub identity: IdentityStrategy,    // UuidV7 (tetap untuk SP1)
}

pub struct FieldMeta {
    pub name: &'static str,
    pub column: &'static str,
    pub kind: FieldKind,   // Char|Text|Integer|Float|Boolean|Date|Timestamptz
                           // |Uuid|Jsonb|M2o{target &str}
    pub nullable: bool,
    pub required: bool,
    pub unique: UniqueScope,           // None | TenantScoped | Global
    pub index: bool,
    pub default: DefaultFn,            // None | Literal(Value) | Fn(fn() -> Value)
    pub selection: Option<&'static [&'static str]>,
    pub audit_scrub: bool,             // #[orm(audit = false)]
}
```

- Slice `linkme`: `MODEL_REGISTRY: [(&ModelMeta, &ModelVtable)]`; `ModelVtable`
  berisi fn ptr (`values_to_row`, `row_to_values`, `validate`).
- `ModelMeta` adalah SATU sumber untuk DDL generator, domain compiler
  (validasi kolom/tipe), dan generator OpenAPI nanti (SP3) — implementator
  dilarang membuat representasi kedua.
