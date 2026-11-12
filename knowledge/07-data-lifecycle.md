# 07 — Cykl życia danych: przyrost, pinning, GC, usuwanie, retencja

Status: **PROPOSED** (z rozmowy z ChatGPT; spójne z dokumentacją IPFS). Dotyczy M9/M10 i części teoretycznej. Przed implementacją zatwierdzić zakres z autorem.

## 1. Fakty o IPFS (źródło: docs.ipfs.tech, persistence, privacy-and-encryption)

- **Nie da się globalnie usunąć CID z sieci IPFS.** Można jedynie przestać przechowywać i ogłaszać go na własnych (kontrolowanych) węzłach. Peer, który pobrał kopię, może ją trzymać.
- DHT nie zawiera plików, tylko rekordy „kto dostarcza multihash X”.
- **Pinning** = „nie usuwaj przy GC”. Niepinowane bloki to cache.
- Kubo: `Datastore.StorageMax`, `StorageGCWatermark` (domyślnie 90% StorageMax), `GCPeriod`. GC usuwa tylko niepinowane bloki.
- Zapytania DHT są publiczne: obserwator może wiedzieć, kto pyta lub dostarcza dany CID.

## 2. Model dla EHR

```text
Repozytorium węzła
├── PINNED   = odpowiedzialność węzła: aktywne rekordy, historia w okresie retencji, wymagane repliki → KEEP
└── UNPINNED = cache: obce bloki z Bitswap, tymczasowe DAG-i → GC
```

- Odpowiedzialność za pin: pacjent (właściciel) PIN, placówka (twórca) PIN, opcjonalny węzeł-replika PIN, losowy peer tylko cache.
- `replication_factor` 2–3, nie „każdy trzyma wszystko”.
- Awans na węzeł-replikę: wolne miejsce + stabilna łączność + zgoda + autoryzacja (spójne z warunkami Client→Server w `05-...` §5). Obce publiczne peery **nie** są zaufanymi replikami.

## 3. Rekord EHR ≠ CID

```rust
enum RecordState { Active, Superseded, Archived, PendingDeletion, Deleted }
struct EhrRecord {
    record_id: RecordId,      // logiczny identyfikator dokumentu
    cid: Cid,                 // konkretna, niezmienna wersja treści (szyfrogramu)
    created_at, retention_until: Timestamp,
    state: RecordState,
    encryption_key_id: KeyId,
    owner: PeerId, creator: PeerId,
    required_replicas: u8,
}
```

- Korekta dokumentu tworzy nową wersję z nowym CID (`v1 CID A → Superseded`, `v2 CID B → Active`). Historia zostaje dla audytu, co jest zgodne z AD-13 (bez IPNS).

## 4. Usuwanie

`DELETE` → sprawdzenie autoryzacji (UCAN) → sprawdzenie retencji → **tombstone** → usunięcie z indeksu EHR → unpin na kontrolowanych węzłach → wstrzymanie replikacji i `stop_providing` → **zniszczenie klucza szyfrującego (crypto-erasure)** → GC.
Crypto-erasure sprawia, że kopie szyfrogramu u obcych peerów są bezużyteczne. Dlatego treść **nigdy nie trafia do IPFS jako jawny tekst** (warstwa GPG, `06-...`).

## 5. Pełny dysk ≠ powód do usuwania dokumentacji

Przy zapełnieniu dysku: najpierw GC cache, potem przeniesienie lub replikacja, potem więcej miejsca. **Nigdy „usuń najstarsze EHR”.**
Prawo PL: dokumentacja medyczna jest przechowywana zwykle **20 lat** od końca roku ostatniego wpisu (wyjątki: np. 30 lat, 10 lat dla niektórych zdjęć RTG). Po okresie retencji niszczy się ją w sposób uniemożliwiający identyfikację pacjenta. W prototypie retencja jest **konfigurowalną polityką**. Przepisy zweryfikować przed cytowaniem w pracy (ustawa o prawach pacjenta i RPP).

## 6. Wymagania dla kodu (gdy przyjdzie czas)

- `ipfs-store`: `delete`, `list` (też do `announce_all` po restarcie), zbiór pinów (osobny prefiks klucza, np. `/pins/`), GC usuwający niepinowane `/blocks/`.
- `ehr-core`: `EhrRecord`, stany, wersje, polityka retencji.
- Nazwy w DAG i manifestach jawnych muszą być **nieprzezroczyste** (bez `/Jan_Kowalski/HIV_test.pdf`).

## 7. Zdanie do pracy (propozycja)

> Obiekty EHR są niezmiennymi, zaszyfrowanymi obiektami IPFS. Trwałość zapewnia pinning i aplikacyjna polityka replikacji; dane tymczasowe pozostają niepinowane i podlegają GC. Rekordy medyczne są przechowywane zgodnie z polityką retencji i nie mogą być usunięte z powodu braku miejsca. Gdy usunięcie jest dozwolone, aplikacja usuwa logiczne odwołanie, cofa autoryzację, niszczy klucz szyfrujący, zdejmuje piny ze wszystkich kontrolowanych replik i pozwala GC odzyskać bloki. Ponieważ IPFS jest zdecentralizowany, usunięcia z niekontrolowanych węzłów nie da się zagwarantować; dlatego treści medyczne nigdy nie są przechowywane jawnie.
