# Phase 9 — Dostarczenie wyniku badania pacjentowi: plan implementacji (M9)

> **Stan:** 18.11.2026. To jest **plan dla osoby (lub agenta), która będzie to implementować**. Nic z sekcji „Do zbudowania” nie istnieje w kodzie. Fragmenty kodu to **szkice nieskompilowane**, pokazują intencję i kształt API, nie gotowe rozwiązanie. Miejsca oznaczone *(do weryfikacji)* trzeba sprawdzić w kodzie lub dokumentacji biblioteki przed użyciem.
>
> Ten dokument łączy: `docs/security-ucan-gpg-integration.md` (UCAN + OpenPGP), `docs/phase-8-unixfs.md` (pliki w IPFS) i `docs/phase-7-bitswap-client.md` (pobieranie). Czytaj je w tej kolejności, jeśli czegoś brakuje.

## 1. Cel i założenia (decyzje autora, nie do zmiany bez pytania)

**Cel:** lekarz wystawia pacjentowi wynik badania (plik), a pacjent bez znajomości jakiegokolwiek CID **widzi go na liście pod nazwą pliku**, może go odczytać i pobrać. Nic więcej.

| # | Założenie |
|---|---|
| Z1 | **Placówka i lekarz to jeden węzeł** (uproszczenie na potrzeby pracy dyplomowej). Jeden klucz, jeden adres, jeden korzeń zaufania. |
| Z2 | **Pacjent ma tylko odczyt i pobranie.** Nie wystawia, nie zmienia, nie udostępnia, nie nadaje uprawnień nikomu (w tym lekarzowi). Tylko lekarz pisze, zmienia, czyta, pobiera, wysyła. |
| Z3 | **Szyfrowanie przed haszowaniem:** plik jest podpisany i zaszyfrowany (OpenPGP), dopiero szyfrogram trafia do `ipfs_unixfs::add_file`. Crate'y `ipfs-*` nie wiedzą o kluczach. |
| Z4 | **Tylko stały chunker** (`size-262144`). Rabin jest do usunięcia: szyfrogram jest losowy, więc Rabin nic nie daje, a kosztuje czas i bloki. Uzasadnienie: sekcja 8 `security-ucan-gpg-integration.md`. |
| Z5 | **Pacjent nie szuka niczego.** Adres węzła lekarza/placówki dostaje na „wizytówce” przy rejestracji i z niego korzysta jako z bootstrapu i dostawcy. Żadnego wyszukiwania w DHT. |
| Z6 | **Pacjent widzi nazwy plików, nie CID.** Jeden indeks (sekcja 6) mapuje nazwę pliku na CID; CID nie pojawia się w interfejsie użytkownika. |
| Z7 | **Imiona, nazwiska i nazwy plików nigdy nie trafiają do IPFS ani DHT otwartym tekstem.** Nazwa pliku jest wewnątrz zaszyfrowanego manifestu (sekcja 5.2). |
| Z8 | Szyfrowanie klucza prywatnego węzła, odzyskiwanie po utracie telefonu, unieważnianie delegacji w sieci: **poza zakresem**. |

## 2. Co już działa (nie przepisywać, używać)

| Warstwa | Crate | Co daje | Użyj do |
|---|---|---|---|
| CID | `ipfs-cid` | `Cid`, CIDv1 SHA2-256, base32 | identyfikacja bloków |
| Magazyn | `ipfs-store` | `PersistentBlockstore` (LevelDB, `put_block` weryfikuje skrót), przestrzenie nazw `/blocks/ /pins/ /provides/ ...` | bloki; **wzór** dla nowych baz |
| UnixFS | `ipfs-unixfs` | `add_file(store, reader, Chunker)`, `cat(store, root, writer)` z limitem 8 GiB i walidacją rozmiarów | zapis/odczyt szyfrogramu |
| Bitswap | `ipfs-bitswap` | serwer wydaje bloki ogłoszone przez `provide`; klient `fetch_many` pobiera falami po 64 bloki | przesył bloków |
| Sieć | `ipfs-network` | `NetworkNode`, DHT (Kademlia, LAN), `provide`/`find`/`fetch`, identyfikacja klucza węzła (`identity::load_or_create`, `peer_id_from_keypair`) | węzeł, połączenia |
| CLI | `ehr-cli` | `add`, `get`, `has`, `node start` + komendy stdin (`provide`, `fetch`, `stop`, `peers`, ...) | punkt wejścia |

Zasady, które łatwo złamać (przeczytaj przed zmianą kodu):

- **LevelDB pozwala na jeden otwarty uchwyt na katalog.** Kod otwiera bazę na czas jednej operacji i ponawia przez 5 s przy blokadzie. Długie operacje (cały DAG, `add` dużego pliku) trzymają blokadę; zlecenia `provide` dużych plików dawaj po kolei. Każda **nowa** baza (kontakty, indeks) ma **własny katalog**, żeby nie rywalizować z blockstore.
- **`fetch` pyta połączonych peerów** po dostawcach z DHT. Przy jednym zaufanym węźle wystarczy, że pacjent jest z nim połączony.
- **`cat` odrzuca DAG bez `blocksizes`** i z niezgodnymi rozmiarami. Nie buduj bloków dag-pb ręcznie; używaj `add_file`.
- **Ręcznie zbudowany DAG, nazwy w manifeście, PESEL:** żadne dane osobowe w blokach jawnych (blok jest widoczny dla każdego, kto zna CID i połączy się z dostawcą).
- Biblioteka `libp2p-stream` jest użyta w **wersji z poprawką** z `vendor/libp2p-stream` (kolejka strumieni 256 zamiast 1). Nie usuwaj `[patch.crates-io]`, bo seria strumieni od Kubo jest gubiona.
- Test-runner uruchamia binarkę `target/debug/ehr-cli`; przed `cargo test -p test-runner` wykonaj `cargo build -p ehr-cli`. Cała reszta: `./scripts/testing/run-rust-tests.sh` (fmt, check, clippy `-D warnings`, testy). **Wszystkie testy muszą przechodzić.**

## 3. Architektura

```text
 JEDNORAZOWO, PRZY REJESTRACJI (osobiście)
 ┌─────────────── PACJENT ───────────────┐            ┌───── LEKARZ + PLACÓWKA (jeden węzeł) ─────┐
 │ klucz węzła -> DID + PeerId           │            │ klucz węzła, adres /ip4/X/tcp/4101/p2p/<D> │
 │ wizytówka pacjenta:                   │ QR / tekst │ data/contacts.leveldb:                     │
 │   DID + publiczny klucz OpenPGP       │ ─────────► │   DID -> imię, nazwisko, klucz PGP         │
 │ wizytówka lekarza:                    │ ◄───────── │ łańcuch /ehr/doc/issue (sam sobie)         │
 │   adres węzła lekarza                 │            └────────────────────────────────────────────┘
 └───────────────────────────────────────┘

 BADANIE WYKONANE (węzeł lekarza)
   1. podpis OpenPGP lekarza -> szyfrowanie dla {pacjent, lekarz} -> szyfrogram
   2. add_file (stały chunker)                      -> CID_doc
   3. token UCAN /ehr/doc/read, aud = DID pacjenta  -> CID_ucan
   4. manifest (ZASZYFROWANY dla pacjenta i lekarza) {nazwa pliku, CID_doc, ...} -> CID_manifest
   5. provide: bloki dokumentu, tokenu, manifestu
   6. wpis w indeksie lekarza (sekcja 6) + skrzynka pacjenta

 PACJENT OTWIERA APLIKACJĘ (albo sprawdza co N sekund)
   PACJENT ──── dzwoni do węzła lekarza (adres z wizytówki) ────► LEKARZ
   /ehr/inbox/1.0.0: „co masz dla DID <ja>?”
   ◄──────── podpisana lista [wpis indeksu: CID_manifest, utworzono, ...] ────────
   weryfikacja podpisu odpowiedzi i tokenu (aud = ja, exp)
   fetch CID_manifest -> odszyfrowanie -> nazwa pliku -> wpis w indeksie pacjenta
   „Wynik krwi 2026-11-18.pdf” widoczny na liście
   doc open „Wynik krwi...” -> fetch CID_doc -> odszyfrowanie -> weryfikacja podpisu -> plik
```

Odpowiedzialność crate'ów (zależności tylko w dół, żaden `ipfs-*` nie zależy od `ehr-*`):

```text
apps/ehr-cli              komendy: contact, doc, inbox, identity; serwisy
   │
crates/ehr-core           manifest, indeks plików, protokół skrzynki (logika), kontakty
   │
crates/ehr-security       did:key, UCAN (token, łańcuch), OpenPGP, lista zaufanych
   │
ipfs-unixfs / ipfs-bitswap / ipfs-network / ipfs-store / ipfs-cid   BEZ ZMIAN (poza punktem 4.5)
```

## 4. Do zbudowania (w tej kolejności)

Każdy krok ma własne testy i kończy się zielonym `run-rust-tests.sh`. Nie przechodź dalej z czerwonym.

### 4.1. `ehr-security::identity` i wizytówka

- `did_from_keypair(&Keypair) -> String` (`did:key:z6Mk...`: `z` + base58btc(`0xed 0x01` + 32 bajty klucza)), `public_key_from_did(&str)`; odwracalne z PeerId.
- Wizytówka: struktura serializowana do tekstu (JSON w base64) z polami `did`, `pgp_public` (armored), `binding` (podpis kluczem węzła nad `did + odcisk PGP`), opcjonalnie `multiaddr`. Weryfikacja: `binding` poprawny dla klucza z `did`.
- CLI: `ehr-cli identity did`, `ehr-cli identity contact` (drukuje wizytówkę), `ehr-cli contact add <wizytówka> --name "Jan Kowalski"`.

```rust
// szkic
pub struct ContactCard { pub did: String, pub pgp_public: String, pub binding: Vec<u8>, pub multiaddr: Option<String> }
impl ContactCard {
    pub fn verify(&self) -> Result<(), SecurityError>; // binding podpisany kluczem z `did`
}
```

Test: wizytówka ze zmienionym bajtem `pgp_public` jest odrzucona.

### 4.2. `ehr-security::pgp` (szyfrowanie)

- Rekomendacja: crate `pgp` (rPGP), czysty Rust *(do weryfikacji: wersja i API `MessageBuilder`, czy szyfruje strumieniowo, czy buforuje cały plik w pamięci)*.
- `encrypt_and_sign(plaintext, signer_secret, recipients: &[PublicKey]) -> Vec<u8>` i `decrypt_and_verify(ciphertext, my_secret, expected_signer_public) -> Vec<u8>`.
- Wynik **jest zwykłymi bajtami**; do `add_file` idzie z `Chunker::fixed()`.

Test: szyfrogram z aplikacji da się odszyfrować zwykłym `gpg --decrypt` i odwrotnie; ten sam plik zaszyfrowany dwa razy daje różne CID (oczekiwane).

### 4.3. `ehr-security::ucan` (token i weryfikator)

- Własny, minimalny weryfikator (decyzja w `security-ucan-gpg-integration.md`, sekcja 4.2): pola `iss`, `aud`, `sub`, `cmd`, `pol`, `exp`, `nbf`, `nonce`, podpis kluczem węzła (`libp2p-identity`).
- Polecenia tylko: `/ehr/doc/issue` (lekarz, korzeń = własny klucz węzła placówki, lista zaufanych), `/ehr/doc/read` (od lekarza dla pacjenta).
- **Reguła Z2:** token `read` jest ważny tylko, gdy `iss` ma własny łańcuch `issue`. Token podpisany przez pacjenta jest odrzucany.
- Testy U01–U11 z `security-ucan-gpg-integration.md`, sekcja 9.

### 4.4. `ehr-core`: manifest i indeks

Manifest (sekcja 5.2) i indeks (sekcja 6). Zastąp szablon `ehr-core::add`. Baza indeksu i kontaktów: **osobne katalogi LevelDB** (`data/index.leveldb`, `data/contacts.leveldb`) otwierane przez istniejący `LevelDbDatastore`, z własnymi prefiksami (wzór: `ipfs-store/src/namespace.rs`).

### 4.5. Protokół skrzynki `/ehr/inbox/1.0.0`

Mały protokół „pytanie, odpowiedź” na strumieniu libp2p (jak Bitswap używa `libp2p-stream`). *(Do weryfikacji: czy dodać drugi `libp2p_stream::Behaviour` do `NetworkBehaviour` w `ipfs-network`, czy użyć `libp2p-request-response`; zmiana w `ipfs-network` ma się ograniczyć do wystawienia generycznego uchwytu strumieni, a logika protokołu leży w `ehr-core`.)*

```text
Żądanie  (pacjent -> lekarz): { patient_did, since_seq: u64, podpis pacjenta nad (did, since_seq, nonce) }
Odpowiedź (lekarz -> pacjent): { entries: [ { seq, manifest_cid, created_utc } ... ], podpis lekarza }
```

- Lekarz odpowiada **wyłącznie** wpisami dla DID z żądania, po sprawdzeniu podpisu pacjenta (dowód, że pyta właściciel DID, a nie ktoś, kto zna DID).
- Pacjent sprawdza podpis lekarza i że `sub` w manifeście to jego DID.
- Limit: maks. N wpisów na odpowiedź i rozmiar wiadomości (jak `MAX_MESSAGE_BYTES` w Bitswap).
- **Nazw plików nie ma w odpowiedzi**: są w zaszyfrowanym manifeście, który pacjent pobiera przez `fetch`. Odpowiedź zawiera tylko CID (kanał libp2p jest szyfrowany Noise, ale nie zakładamy, że odpowiedź jest tajna).

### 4.6. Komendy `doc` w CLI

Podział (decyzja DEC1, sekcja 11): komendy wymagające sieci i bazy bloków działają **wewnątrz uruchomionego węzła** jako polecenia stdin (jak dziś `provide`, `fetch`); komendy tylko lokalne są zwykłymi komendami CLI. Dzięki temu jest jeden właściciel bazy bloków i nie ma rywalizacji o blokadę LevelDB.

```text
# polecenia stdin w uruchomionym węźle (`ehr-cli node start ...`)
doc issue <plik> --patient <DID|nazwisko> [--name "Wynik krwi.pdf"]   # tylko węzeł lekarza
doc update <numer|doc_id> <plik>                                      # nowa wersja
inbox sync                                                            # pacjent: zapytaj lekarza, pobierz manifesty, uzupełnij indeks
doc open <numer|nazwa> <plik-wyjściowy>                               # fetch + odszyfrowanie + weryfikacja + zapis

# zwykłe komendy CLI (offline)
ehr-cli identity did | contact | init     # init: tworzy klucz PGP (sekcja 11, DEC2)
ehr-cli contact add <wizytówka> [--name "Jan Kowalski"]
ehr-cli doc list [--patient <DID|nazwisko>]                           # tabela z nazwami (sekcja 6)
```

## 5. Formaty danych

### 5.1. Co jest w IPFS (jawne bloki widoczne dla każdego z CID)

| Blok | Zawartość | Dane osobowe? |
|---|---|---|
| szyfrogram dokumentu (UnixFS) | losowe bajty | nie |
| manifest (UnixFS, **szyfrogram**) | patrz 5.2, zaszyfrowany dla pacjenta i lekarza | nie, dopóki nie odszyfrowany |
| token UCAN `read` | `iss`, `aud` (DID), `pol` (CID dokumentu), `exp` | DID są pseudonimami; **bez imion** |

### 5.2. Manifest (po odszyfrowaniu)

```json
{
  "v": 1,
  "doc_id": "bafy...",            // CID pierwszego manifestu w łańcuchu wersji (stały identyfikator dokumentu)
  "file_name": "Wynik krwi 2026-11-18.pdf",
  "media_type": "application/pdf",
  "size": 183422,
  "doc": "bafy...",               // CID szyfrogramu dokumentu
  "sub": "did:key:z6Mk...",       // pacjent
  "iss": "did:key:z6Mk...",       // lekarz
  "iss_name": "Poradnia Kardiologiczna",  // wyświetlana pacjentowi
  "created_utc": 1795000000,
  "prev": null,                   // CID poprzedniej wersji manifestu albo null
  "read_token": "bafy..."         // CID tokenu UCAN
}
```

Zakaz: PESEL, imię i nazwisko pacjenta, rozpoznania w manifeście. `file_name` pochodzi od lekarza i **nie jest ufany jako ścieżka** (sekcja 6.4).

## 6. Jeden indeks plików: nazwy zamiast CID

### 6.1. Po co

Użytkownik (lekarz i pacjent) nigdy nie widzi ani nie wpisuje CID. Widzi listę z nazwami, a program zamienia wybór na CID. Jeden schemat i jeden kod (`ehr-core::index`) dla obu ról: różnią się tylko tym, kto zapisuje wpisy.

### 6.2. Schemat

Baza: `data/index.leveldb` (osobny katalog LevelDB, własna blokada). Klucz: `/index/<DID pacjenta>/<seq w big-endian, 8 bajtów>`; wartość (JSON lub prost):

```text
IndexEntry {
  seq: u64,                // kolejny numer dla tego pacjenta (u lekarza nadawany przy wystawieniu)
  doc_id: String,          // stały identyfikator dokumentu (CID pierwszego manifestu)
  file_name: String,       // do wyświetlenia
  media_type: String,
  size: u64,
  manifest_cid: String,    // bieżąca wersja
  doc_cid: String,         // szyfrogram
  issuer_did: String, issuer_name: String,
  created_utc: u64,
  prev_manifest_cid: Option<String>,
  status: Verified | Unverified   // czy łańcuch UCAN i podpis lekarza zweryfikowane
}
```

### 6.3. Kto i kiedy zapisuje

| Rola | Zdarzenie | Zapis |
|---|---|---|
| Lekarz | `doc issue` / `doc update` | wpis dla DID pacjenta z `file_name` podanym przy wystawieniu; źródło prawdy dla skrzynki (`inbox`) |
| Pacjent | `inbox sync` | po pobraniu i odszyfrowaniu manifestu: wpis z `file_name` z manifestu (nie z odpowiedzi skrzynki) |

Nowa wersja dokumentu to nowy wpis z tym samym `doc_id` i `prev_manifest_cid`. Lista pokazuje **ostatnią wersję** każdego `doc_id`, a historia jest dostępna po numerze.

### 6.4. Wyświetlanie i odczyt

```text
$ ehr-cli doc list --patient "Kowalski"        # lekarz: nazwisko -> DID z contacts.leveldb
 #  Nazwa                              Data        Wystawił               Status
 1  Wynik krwi 2026-11-18.pdf          2026-11-18  Poradnia Kardiologiczna  zweryfikowany
 2  EKG 2026-11-02.pdf                 2026-11-02  Poradnia Kardiologiczna  zweryfikowany

> doc open 1 /home/pacjent/wyniki/krew.pdf             # w węźle; numer albo nazwa; CID nie jest potrzebny
```

- Zapis na dysk: nazwa z manifestu to tylko **podpowiedź**; ścieżkę wyjściową podaje użytkownik. Jeśli program kiedyś użyje `file_name` do ścieżki, ma usunąć separatory, `..` i znaki sterujące (nazwa jest kontrolowana przez wystawcę).
- Zdublowane nazwy: pokaż numer `#`; `open` po nazwie przy niejednoznaczności wypisuje kandydatów zamiast zgadywać.
- Lekarz zamienia nazwisko na DID przez kontakty (`contacts.leveldb`: DID -> imię, nazwisko, klucz PGP, adres). To jedyne miejsce z imionami; plik zostaje lokalny, nigdy nie jest ogłaszany.

## 7. Przykład: cały przebieg w CLI (docelowy)

```text
# --- rejestracja, osobiście ---
pacjent$ ehr-cli identity contact          > karta-pacjenta.txt      # DID + klucz PGP
lekarz$  ehr-cli contact add karta-pacjenta.txt --name "Jan Kowalski"
lekarz$  ehr-cli identity contact          > karta-lekarza.txt       # adres węzła lekarza
pacjent$ ehr-cli contact add karta-lekarza.txt                       # zapamiętuje adres lekarza jako bootstrap

# --- badanie wykonane ---
lekarz$  ehr-cli node start --dht-server                             # węzeł działa, dalej polecenia w jego stdin:
lekarz>  doc issue /home/lekarz/krew.pdf --patient "Kowalski" --name "Wynik krwi 2026-11-18.pdf"
         [DOC] ISSUED name="Wynik krwi 2026-11-18.pdf" patient=did:key:z6Mk... manifest=bafy...

# --- pacjent ---
pacjent$ ehr-cli node start                                          # łączy się z węzłem lekarza (adres z kontaktu)
pacjent> inbox sync
         [INBOX] NEW "Wynik krwi 2026-11-18.pdf" verified
pacjent$ ehr-cli doc list                                   # (offline, czyta data/index.leveldb)
          1  Wynik krwi 2026-11-18.pdf  2026-11-18  Poradnia Kardiologiczna  zweryfikowany
pacjent> doc open 1 /home/pacjent/krew.pdf
```

## 8. Testy do napisania

| Poziom | Co |
|---|---|
| Jednostkowe `ehr-security` | wizytówka (poprawna, zmieniona), did:key <-> PeerId, szyfrowanie wielu odbiorców, odszyfrowanie `gpg`, U01–U11 (UCAN) |
| Jednostkowe `ehr-core` | manifest (kodowanie), indeks (kolejność `seq`, ostatnia wersja per `doc_id`, zdublowane nazwy), odpowiedź skrzynki tylko dla właściwego DID |
| Integracyjne (`test-runner`) | trzy procesy `ehr-cli`: lekarz, pacjent A, pacjent B. Pacjent A po `inbox sync` widzi tylko swoje pliki; B nie widzi plików A (T05); B z CID szyfrogramu A nie odszyfruje; pacjent nie wystawi ani nie przekaże dokumentu (T08, U09, U10) |
| Interop | szyfrogram otwiera się w `gpg`; CID szyfrogramu zgodny z `ipfs add --cid-version=1 --raw-leaves --chunker=size-262144` |

## 9. Zasady pracy (jak w reszcie repozytorium)

- Najpierw plan kroku w `docs/`, potem kod; po każdym kroku zaktualizuj odpowiedni dokument (tabela testów, ograniczenia, zabezpieczenia).
- Komentarze i dokumentacja po polsku, nazwy w kodzie po angielsku; komentarz tylko tam, gdzie wyjaśnia *dlaczego*.
- Skrót z ograniczeniem opisz komentarzem `shortcut: <ograniczenie>, <kiedy ulepszyć>`.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` i skrypty `scripts/testing/*.sh` muszą przechodzić (`test-network.sh` wymaga **osobnego** repozytorium Kubo w `IPFS_PATH`: zatrzymuje demona z tego katalogu).
- Nie dodawaj zależności bez uzasadnienia; nowe crate'y z wersją sprawdzoną kompilacją z `libp2p 0.57.1`.

## 10. Znane ograniczenia tego planu

- Pacjent dostaje plik tylko, gdy węzeł lekarza jest włączony i osiągalny (jeden węzeł, brak kopii zapasowej).
- Węzeł lekarza musi mieć adres osiągalny dla pacjenta (publiczny IP albo przekierowanie portu); pacjent za NAT sam dzwoni, więc to nie przeszkadza.
- Utrata klucza prywatnego pacjenta oznacza utratę dostępu do dokumentów (Z8: poza zakresem).
- Cofnięcie dostępu nie odbiera pobranych kopii.
- `status: Unverified` pojawia się, gdy łańcuch UCAN lub podpis lekarza nie przeszły weryfikacji; interfejs musi to pokazać, a nie ukrywać.

## 11. Decyzje (rozstrzygnięte; nie pytaj ponownie, chyba że kod temu przeczy)

| ID | Decyzja |
|---|---|
| DEC1 | `doc issue`, `doc update`, `inbox sync`, `doc open` to **polecenia stdin węzła** (wzór: `stdin_commands` w `apps/ehr-cli/src/main.rs`). `doc list`, `contact add`, `identity ...` to zwykłe komendy offline. Powód: węzeł jest jedynym właścicielem bazy bloków (problem blokady LevelDB). Odpowiedzi wypisuj jako linie `[DOC] ...`, `[INBOX] ...`, `[DOC_FAILED] ...`, żeby `test-runner` mógł czekać na wzorzec (zob. `test-runner/src/node_process.rs`). |
| DEC2 | Klucz PGP generuje `ehr-cli identity init` w `data/keys/pgp-secret.asc` (katalog `0700`, plik `0600`), bez hasła (Z8). Klucz węzła zostaje tam, gdzie jest (`./tmp/node-key.pb`). |
| DEC3 | `contact add` z wizytówką **lekarza** zapisuje jego adres i DID w `data/contacts.leveldb` pod prefiksem `/doctor/`. `node start` bez argumentów bootstrapu używa tego adresu zamiast domyślnego (zmiana w `NetworkService::node_config`); DID lekarza trafia na listę zaufanych wystawców. |
| DEC4 | Wizytówka, manifest, token UCAN i wiadomości skrzynki: **JSON (`serde_json`)**. Podpis liczony nad **dokładnymi bajtami ładunku** (`payload` jako base64 + `signature` obok); nie ma kanonikalizacji JSON. Manifest po zaszyfrowaniu jest zwykłym plikiem UnixFS. |
| DEC5 | Protokół skrzynki: `libp2p-stream` (już w projekcie, w wersji z poprawką), drugi protokół obok Bitswap. W `ipfs-network` wolno dodać **wyłącznie** generyczny uchwyt strumieni (`NetworkNode::stream_control()`); logika protokołu leży w `ehr-core`. Wzór: `ipfs-bitswap/src/models.rs` i `service.rs`. *(do weryfikacji szczegóły `Behaviour`/`Control`)* |
| DEC6 | Korzeń zaufania to klucz węzła lekarza (token `issue` wystawiony samemu sobie). Placówka = lekarz. Dostęp innego lekarza (`doc grant`): poza zakresem. |
| DEC7 | Dokumenty podpisuje lekarz (OpenPGP). Pacjent podpisuje tylko żądanie skrzynki. Pliki zaszyfrowane importuj **zawsze** `Chunker::fixed()`; usunięcie Rabina z kodu to osobny krok na końcu (sekcja 12, krok R). |
| DEC8 | Nowe katalogi baz: `data/contacts.leveldb`, `data/index.leveldb` (każdy z własną blokadą). Wzór otwierania: `LevelDbDatastore::open`. |

## 12. Kolejność prac i kryteria ukończenia

Każdy krok kończy się: `./scripts/testing/run-rust-tests.sh` na zielono (fmt, clippy `-D warnings`, wszystkie testy), aktualizacją tego dokumentu (stan kroku, testy, ograniczenia) i krótkim podsumowaniem dla autora. **Nie rób commitów ani pushy, dopóki autor o to nie poprosi.**

| Krok | Zakres | Gotowe, gdy |
|---|---|---|
| 1 | `crates/ehr-security`: `identity` (did:key, wizytówka), `ehr-cli identity did/contact/init` | testy wizytówki i did <-> PeerId; komendy działają offline |
| 2 | `ehr-security::pgp`: klucze, szyfrowanie+podpis, odszyfrowanie+weryfikacja | test z `gpg --decrypt` w obie strony; różne CID przy powtórnym szyfrowaniu |
| 3 | `ehr-security::ucan`: token, weryfikator łańcucha, lista zaufanych | U01–U11 zielone |
| 4 | `ehr-core`: manifest, indeks (`index.leveldb`), kontakty (`contacts.leveldb`), `contact add`, `doc list` | testy z sekcji 8 (jednostkowe `ehr-core`) |
| 5 | `/ehr/inbox/1.0.0` w `ehr-core` + uchwyt strumieni w `ipfs-network` | test dwóch węzłów: żądanie podpisane, odpowiedź tylko dla właściciela DID |
| 6 | polecenia węzła: `doc issue`, `inbox sync`, `doc open`, `doc update` | test integracyjny z 3 procesami (lekarz, pacjent A, pacjent B) z sekcji 8 |
| R | usunięcie Rabina z `ipfs-unixfs` i testów (zakres w `security-ucan-gpg-integration.md`, sekcja 8) | wszystkie testy zielone, CID zgodne z Kubo dla `size-262144` |

Zasada postępowania: jeśli decyzja nie jest w sekcjach 1 lub 11 i zmienia architekturę, **zatrzymaj się i zapytaj autora** zamiast zgadywać. Mniejsze wybory (nazwy, układ modułów) podejmij sam i opisz w dokumencie.

## 13. Prompt startowy dla agenta (wklej na początku sesji)

```text
Pracujesz w repozytorium /home/andrew/ipfs/ipfs_prototype (Rust, workspace, gałąź wskazana przez autora).
Zadanie: zaimplementować dostarczenie wyniku badania pacjentowi wg docs/phase-9-ehr-delivery-plan.md.

Przeczytaj w tej kolejności: docs/phase-9-ehr-delivery-plan.md (całość, sekcje 1, 2, 11, 12 są wiążące),
docs/security-ucan-gpg-integration.md (UCAN, OpenPGP), docs/phase-8-unixfs.md, docs/phase-7-bitswap-client.md.
Przy sprzeczności wygrywa phase-9.

Zasady:
- Pracuj krok po kroku wg sekcji 12. Nie przechodź dalej z czerwonymi testami.
- Nie przepisuj crate'ów ipfs-*; wyjątek: generyczny uchwyt strumieni w ipfs-network (DEC5).
- Dokumentację i komentarze pisz po polsku, nazwy w kodzie po angielsku; komentarz tylko, gdy wyjaśnia "dlaczego".
- Przed zakończeniem kroku: ./scripts/testing/run-rust-tests.sh musi przejść; przy zmianach sieci uruchom też
  scripts/testing/test-kubo-bitswap-interop.sh i test-kubo-unixfs-interop.sh.
- Nie rób commitów ani pushy. Nie ruszaj vendor/libp2p-stream ani [patch.crates-io].
- Rzeczy oznaczone "(do weryfikacji)" sprawdź w kodzie lub dokumentacji biblioteki i zapisz wynik w dokumencie.
- Jeśli decyzja zmienia architekturę i nie ma jej w dokumencie, zatrzymaj się i zapytaj.
Zacznij od kroku 1 i po jego zakończeniu podsumuj: co zrobiono, jakie testy przechodzą, co jest niezweryfikowane.
```
