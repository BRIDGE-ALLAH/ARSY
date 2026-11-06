# ARSY — Arsitektur Platform

> Odoo-style ORM, Supabase-style multi-tenant BaaS, dibangun di Rust.
> Dokumen ini adalah kontrak arsitektur; tiap sub-proyek punya spesifikasi sendiri
> di `docs/superpowers/specs/`.

## 1. Visi

ARSY adalah platform data aplikasi: developer mendeklarasikan **model** seperti
mendeklarasikan model Odoo (field typed, default data, add-on), dan secara otomatis
memperoleh **REST API multi-tenant yang aman** seperti Supabase — termasuk auth,
RLS, audit trail, storage, dan dokumentasi OpenAPI. Jalur evolusi:
monolith self-host → BaaS → SaaS.

## 2. Keputusan teknologi terkunci

| Area | Keputusan | Alasan singkat |
|---|---|---|
| Bahasa/runtime | Rust, workspace multi-crate, toolchain dipin | satu disiplin, performa |
| DB | **PostgreSQL only**, via `sqlx` | Odoo-class ORM + RLS adalah fitur server |
| Tenancy | **Shared schema + kolom `tenant_id` + PostgreSQL RLS** (BUKAN schema-per-tenant) | model Supabase; provisioning simpel, migrasi sekali jalan |
| Object storage | **Cloudflare R2** (S3-compatible), satu bucket di awal; trait `Storage` + backend `fs`/`mem` untuk dev/test | egress gratis; jalur BYO-bucket saat enterprise |
| ID record | **UUIDv7** | time-sortable, tidak membocorkan volume antar-tenant |
| API | Router **generatif** dari model registry, `/rest/v1/*`, OpenAPI 3 otomatis + Scalar | kontrak = produk BaaS |
| Auth | JWT → `ActorContext`; API-key per tenant; argon2; role `platform_admin`/`tenant_admin` | identitas adalah hulu RLS/audit |
| Konsol admin (Studio) | **Next.js** (`web/arsy-console`), TanStack Table/Form + shadcn, memakai SDK TS yang dipublish platform (dogfood) | kualitas UI produk; aturan: **tidak ada logika bisnis di console** |
| SDK tenant | TypeScript client digenerate dari OpenAPI (`openapi-typescript`) | app milik tenant adalah urusan tenant |
| Add-on | Crate per add-on (statis) + manifest deklaratif + **data-file deklaratif**; state install global di `ir_module`, enablement per tenant | Odoo rasa Rust; hot-install deklaratif bisa menyusul |
| Kontrak antar-lapisan | Rust = sumber kebenaran; web hanya konsumen; jembatan satu-satunya `/rest/*` | mencegah dua backend |

## 3. Peta workspace target

```
crates/
  arsy-orm/          Model trait+metadata, Field, Domain→SQL, ModuleRegistry,
                     DDL+RLS policy generator, ActorContext, UnitOfWork,
                     automatic columns, audit_log, default-data loader
  arsy-orm-macros/   #[derive(Model)], #[orm(...)], domain! macro
  arsy-storage/      trait Storage; backends r2 / fs / mem
  arsy-api/          router generatif /rest/*, OpenAPI+Scalar, middleware auth,
                     kuota, pagination, select/expands
  arsy-console?      (tidak ada — console bukan crate Rust)
  arsy-cli/          arsy serve / migrate / module install / tenant create
addons/
  base/              res.Company, res.User, res.Partner, ir.Module,
                     ir.Attachment, audit_log, default data + ACL dasar
  (sale/, .../)      add-on turunan; _extend model base
apps/
  arsy-server/       wiring: config → registry → router (tetap crate aplikasi)
web/
  arsy-console/      Next.js Studio + auth pages (SP7+)
docs/superpowers/specs/   spesifikasi per sub-proyek
```

Arahan dependensi: `arsy-api → arsy-orm → (sqlx)`, `arsy-storage` dan `arsy-orm`
saling bebas; model = unit registrasi, attachment = model ORM biasa.

## 4. Prinsip data platform (invariant)

1. **Semua write melewati ORM.** Tidak ada SQL langsung dari lapisan API; audit,
   automatic columns, dan tenant scoping bergantung padanya.
2. **Setiap operasi ORM menerima `ActorContext`** — tipe-opsional mustahil;
   lupa = compile error. `System` actor dibatasi role `BYPASSRLS`.
3. **Semua tabel model punya `tenant_id`** (diisi otomatis), `created_uid/at`,
   `write_uid/at`. Kolom ini tidak dapat ditulis kode bisnis.
4. **Satu request = satu Unit of Work = satu transaksi Postgres**; commit di
   akhir router; `SET LOCAL app.tenant_id` per transaksi; audit_log dan default
   data ikut dalam transaksi yang sama (tidak ada state setengah terinstall).
5. **Domain compiler 100% bind-parameter** — tidak pernah string-concat SQL.
   Snapshot test SQL wajib ada.
6. **Cache v1 = read-cache/prefetch saja; write langsung flush.** Cache-buffer
   Odoo-style ditunda sampai ada bukti kebutuhan.
7. **`ir_module` global; konfigurasi feature/API-key/quota per tenant.**

## 5. RLS & tenancy

- Setiap tabel model dibuat dengan policy
  `USING (tenant_id = current_setting('app.tenant_id')::uuid)`.
- Connection pool: routing tenant via `SET LOCAL` dalam transaksi (bukan
  per-connection state), wajib ada guard agar statement di luar transaksi
  tidak melihat nilai request sebelumnya.
- `ir_tenant` registry: id, name, status (active/suspended/deleted), kuota
  (max rows, max storage bytes, rate).
- Siklus hidup tenant: provision → suspend → **hard delete = purge**
  (baris via RLS-scoped job, object R2 lewat GC, retention audit_log
  mengikuti kebijakan) — jalur GDPR.

## 6. Add-on & default data

- Manifest di `[package.metadata.arsy.addon]`: `name`, `version`,
  `depends = ["base"]`, `data = ["data/company.toml", ...]`, `install_hooks`.
- Registry mengumpulkan model saat binary start melalui registrasi link-time
  (`linkme`/`inventory`); setiap model punya field `owner_addon`.
- Install pipeline (per transisi `to install` → `installed`), dalam 1 transaksi:
  topologis urut depends → DDL create/alter → muat data default → hooks.
- **External ID** `module.name` (kolom `xml_id` unik) → upsert idempotent;
  `noupdate = true` tidak ditimpa saat upgrade; add-on lain boleh
  me-reference `base.company_root`.
- `_inherit`/extend: merge field dari add-on pemakai ke model owner lewat
  registry (SP7), bukan pewarisan bahasa.
- Migrasi per add-on: `migrations/<version>/*.sql`, dijalankan bila
  `ir_module.latest_version` tertinggal.

## 7. Auth

- Password: argon2id; session: JWT short-lived + refresh; API-key per tenant
  (header `x-arsy-key`); service role untuk server-to-server.
- JWT claim `{sub, tenant, actor_type, roles}` → diterjemahkan middleware
  menjadi `ActorContext`; tidak ada jalur auth kedua untuk console
  (admin login memakai mekanisme yang sama, role `platform_admin`).
- Signup/verifikasi/reset email lewat provider webhook (SP5), bukan SMTP
  internal.

## 8. Storage & attachment

- `ir.Attachment` = model ORM: `name`, `res_model`, `res_id` (polymorphic),
  `mimetype`, `file_size`, `checksum(sha256)`, `storage_key`, `public`.
- Kunci objek: `t/{tenant_id}/{res_model}/{res_id}/{ulid}-{slug(name)}`.
- Download: cek RLS → redirect 307 ke signed URL TTL ±60 dtk.
- Upload: proxy lewat server **atau** presigned `sign_upload → put → complete`
  (mode direct-to-R2 wajib tersedia sejak awal supaya scale SaaS murah).
- Delete: record unlink in-tx; objek → antrean `storage_gc` (sweeper
  background); tidak pernah hapus objek sebelum commit.
- Batas ukuran & mime allowlist per tenant dari `ir_tenant`.

## 9. Runtime features (SP5)

- **Scheduler**: `ir.cron` di DB + worker loop (GC storage, retry webhook,
  purge tenant, report kuota).
- **Webhook keluar**: event bus create/write/unlink → dispatch dengan HMAC +
  retry ber-antre (tabel `delivery_queue`).
- **Realtime**: SSE/WebSocket `/rest/live/{model}` berbasis
  `LISTEN/NOTIFY` per perubahan (payload = id + delta ringkas, otorisasi
  dicek ulang per subscriber dengan policy RLS).

## 10. Observability, testing, operasi

- `tracing` (sudah ada) + `/metrics` Prometheus + slow-query log → SP3.
- **Testing strategy** (wajib sejak SP1):
  - unit murni: domain AST, manifest parse, external-id upsert;
  - snapshot: SQL yang digenerate DDL/domain (bind-safe terverifikasi);
  - integrasi: testcontainers-Postgres;
  - **RLS regression suite**: dua tenant saling mencoba akses → wajib kosong,
    menjadi gate CI mulai SP2;
  - kontrak: test `/rest/*` generatif vs OpenAPI spec (schema drift).
- Ops checklist sebelum produksi pertama: backup/PITR Postgres, retention
  audit, secrets manager (bukan `.env` polos di server), connection pool
  sizing, `cargo deny` di CI (sudah aktif), `pnpm audit` saat console lahir.
- Dev lokal: `docker-compose` menyediakan Postgres; backend storage dev =
  `fs`/`mem` (MinIO opsional, bukan default).

## 11. Roadmap sub-proyek

| SP | Isi | Gate selesai |
|---|---|---|
| SP1 | ORM core (derive, field v1, UUIDv7, UoW, ActorContext, audit, default data, ModuleRegistry) + `addons/base`; arsy-server bermigrasi ke ORM; tenant_id ada, RLS off | spec: `docs/superpowers/specs/2026-11-06-arsy-orm-core-design.md` |
| SP2 | RLS ON + provisioning/`ir_tenant` + auth JWT/API-key penuh | RLS regression suite hijau |
| SP3 | Router `/rest/v1` generatif + OpenAPI/Scalar + kuota minimum + **arsy-cli** + metrics | SDK TS tergenerate & dipakai test |
| SP4 | Storage `arsy-storage` + `ir.Attachment` + presigned + GC | upload/download lintas tenant terisolasi |
| SP5 | Scheduler + webhook + realtime | cron GC jalan end-to-end di compose |
| SP6 | Relasi Many2one/O2o/M2M + computed fields | prefetch N+1-free, test query count |
| SP7 | `_inherit` + ACL penuh per model/role | add-on contoh memperluas base tanpa ubah base |
| SP8 | `web/arsy-console` Next.js (Studio + auth pages) | console 100% via SDK, tanpa endpoint privat tambahan |

**Keputusan terbuka** (tidak memblokir SP1): lisensi core (rekomendasi
Apache-2.0 untuk `arsy-orm*` + `addons/base`, platform/SaaS tetap tertutup);
posisi `web/arsy-console` di repo ini atau repo terpisah (default: sama,
`pnpm` workspace sendiri); provider email/transaksional untuk auth.
