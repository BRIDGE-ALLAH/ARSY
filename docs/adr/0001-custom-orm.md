# ADR-0001: ORM kustom, bukan ORM yang ada

- Status: accepted
- Date: 2026-11-06

## Context

ARSY ingin menjadi "Odoo-style ORM, Supabase-style BaaS" di Rust. Pilihan:
memakai SeaORM/Diesel dan menambahkan lapisan Odoo di atasnya, atau menulis
`arsy-orm` dari nol di atas `sqlx`.

## Decision

Membangun ORM kustom (`arsy-orm` + `arsy-orm-macros`) dari dasar. Fitur
kunci platform — ModuleRegistry/add-on install pipeline, DDL + RLS policy
generator, Model::validate hook, external-id default data, automatic
columns, Unit of Work — semuanya adalah perilaku ORM yang tidak dapat
dititipkan rapi pada ORM lain tanpa melawan desain mereka.

## Consequences

- Positive: kontrol penuh atas rasa Odoo dan kontrak BaaS; tidak ada
  impedance mismatch registry-ORM; nilai belajar/produk maksimal.
- Negative: kita adalah maintainer database-mapping layer; bug correctness
  (RLS bocor, lost update, DDL drift) menjadi tanggung jawab penuh.
- Mitigasi: jaring pengaman test SP1 (snapshot SQL, RLS regression suite,
  paritas validasi, konkurensi FOR UPDATE) + bind-only domain compiler +
  invariant "semua write lewat ORM".
- Follow-up: bila di kemudian hari maintenance cost melebihi nilai, lapisan
  internal boleh diganti tanpa mengubah API publik (invariant §4
  ARCHITECTURE.md).
