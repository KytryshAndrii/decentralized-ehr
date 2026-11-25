# Wyjaśnienia ogólne: cykl życia danych, warstwy, FHIR, PeerId, protobuf, Slint, indeks

Stan na 2026-11-25, branch `m9-ehr-fhir-security-ui`.

## Model cyklu życia danych a replikacja (Must fix 1)

Model z `knowledge/07-data-lifecycle.md` dobrze nadaje się do rozdziału pracy o cyklu życia danych.

**Co ten model rozwiązuje.** Wprowadza dwie osobne kategorie przechowywania: przypięte rekordy EHR i nieprzypiętą pamięć podręczną (cache).
- Limit 10 GB w GC dotyczy tylko cache. To już mamy.
- Danych przypiętych nigdy nie usuwa się z powodu pełnego dysku. To też już mamy.
- Nowy pomysł: pacjent pinuje **swoje** dokumenty. Dziś tego nie robi.
- Zbiór replik to 3 węzły: placówka wystawiająca (korzeń), pacjent i jeszcze jeden węzeł placówki.

Jeśli repliki trzymają tylko węzły placówek (opcja (a)), przypięte dane pacjenta to wyłącznie jego własne rekordy. Wtedy nikt nie może zapełnić mu dysku i **problem z Must fix 1 znika**.

**Czego model nie rozwiązuje.** Nie mówi, **kto może poprosić** węzeł o przypięcie danych. Dziś może to zrobić każdy peer z podpisem, a w modelu nie ma kroku autoryzacji. Minimalna poprawka:
1. Tylko program lekarza obsługuje `/ehr/replicate` (jeden warunek w `node.rs:91`).
2. Później UCAN albo rejestr kluczy placówek. Notatki w pamięci odkładają to już na etap „UCAN później”.

**Co poprawić w modelu:**
- **„Maksymalny rozmiar danych przypiętych, z ostrzeżeniem przed usunięciem”** przeczy wnioskowi z tego samego modelu („nie mogą być usunięte z powodu braku miejsca”). Zostawić samo ostrzeżenie, bez usuwania.
- **„Zniszczenie klucza szyfrującego”** nie działa w naszym modelu. OpenPGP szyfruje klucz sesji osobno dla klucza PGP każdego odbiorcy i nie ma osobnego klucza na dokument. Crypto-shredding wymagałby klucza na dokument z `key_id`, trzymanego w osobnym magazynie kluczy.
- **„Sprawdź, czy 2 inne węzły mają CID”** da się zrobić przez `GetProviders`. Rekordy dostawców to jednak deklaracje samych węzłów i wygasają po 48 h, więc to tylko wskazówka.
- **Retencja:** liczymy 20 lat od daty wystawienia (`DOCUMENT_VALIDITY_YEARS`). Prawo liczy od **końca roku kalendarzowego** ostatniego wpisu, z wyjątkami. Przed cytowaniem w pracy sprawdzić art. 29 ustawy o prawach pacjenta.
- **Publiczny DHT:** DHT nie przechowuje danych jawnych, tylko rekordy „CID → PeerId”. PeerId dostawcy jest jednak widoczny, więc da się ustalić, że węzeł X przechowuje CID Y. Te metadane wyciekają.

**Decyzja (wdrożona, wariant a+):** opcja (a) oraz pinowanie przez pacjenta jego własnych dokumentów. To daje 3 kopie (lekarz + pacjent + inny lekarz) i zachowuje limit 10 GB. Szczegóły w sekcji 7.

## 1. Warstwy payloadu: GPG + UCAN + FHIR + DAG

```
Plik (PDF, wideo, tekst…): oryginalne bajty
   │ OpenPGP: podpis (klucz lekarza) + szyfrowanie (dla {pacjent, lekarz})
   ▼
Szyfrogram .pgp
   │ UnixFS: fragmenty 256 KiB → bloki raw + węzły dag-pb (protobuf)
   ▼
DAG dokumentu ── doc_cid (korzeń)
                  │
Token UCAN (odczyt): iss=lekarz, aud=pacjent, cmd=/ehr/doc/read,
                     pol=[doc_cid], exp=+20 lat, prf=[token główny]
                  │
FHIR R5 DocumentReference (JSON): subject=DID pacjenta, author=DID lekarza,
   content.attachment.url = ipfs://doc_cid, title, contentType,
   extension: token odczytu + valid-until, relatesTo: replaces (nowa wersja)
   │ OpenPGP: podpis + szyfrowanie, tak samo jak plik
   ▼
DAG manifestu ── manifest_cid
```

Co płynie przez sieć:

```
Inbox (/ehr/inbox): podpisana ramka JSON {seq, manifest_cid, created_utc}
Bloki (Bitswap): protobuf Message {wantlist | payload[prefix, data]}
       └─ wewnątrz libp2p: Noise (szyfrowanie transportu) + Yamux (multipleksowanie strumieni) po TCP
```

Pacjent idzie od dołu do góry:
1. Dostaje `manifest_cid` z inboxu.
2. Pobiera bloki manifestu przez Bitswap i sprawdza CID każdego bloku.
3. Odszyfrowuje manifest i weryfikuje podpis lekarza.
4. Weryfikuje token UCAN: jest dla mnie, dla tego `doc_cid` i jest ważny.
5. Pobiera DAG dokumentu i składa szyfrogram.
6. Odszyfrowuje go i weryfikuje podpis, co daje oryginalny plik.

## 2. HL7 FHIR i EHR

**HL7 FHIR R5** to standard wymiany danych medycznych w postaci „zasobów”, zwykle JSON przez REST. Przykłady zasobów:
- `Patient`, `Practitioner` (osoby)
- `Observation`, `Condition`, `MedicationRequest` (dane kliniczne)
- `DocumentReference` (wskaźnik na dokument i jego metadane)

**Korzystamy z:**
- **`DocumentReference`** jako manifestu, z polami `status`, `subject`, `author`, `date`, `category`, `content.attachment`, `relatesTo` (wersje) i `identifier`.
- **`Patient` i `Practitioner`** jako wizytówek kontaktów.

**Nie korzystamy z:** serwera FHIR REST ani z ustrukturyzowanych danych klinicznych, takich jak Observation i Condition. Sam dokument jest plikiem, a nie danymi FHIR.

**Po co są stałe w `consts.rs`.** FHIR wymaga, żeby każdy identyfikator i kod miał `system`, czyli przestrzeń nazw mówiącą, co znaczy wartość.
- `urn:ietf:rfc:3986` oznacza „wartość jest URI”, i tak zapisujemy DID.
- `urn:ehr:...` to nasze własne przestrzenie nazw (id dokumentu, kategoria, rozszerzenia dla tokenu i daty ważności), bo FHIR nie ma dla nich standardowych kodów.
- `http://hl7.org/fhir/document-relationship-type` to **oficjalny identyfikator systemu kodów HL7**. Nikt go nie pobiera przez sieć. To nazwa, a nie zapytanie, ale bez niej `relatesTo.code = replaces` nie byłoby poprawnym FHIR.

**Czym jest EHR.** EHR (Electronic Health Record) to elektroniczna dokumentacja pacjenta, prowadzona w czasie i wspólna dla wielu placówek. Wzorcowym standardem wymagań dla systemów EHR jest ISO 18308. Do wymiany używa się HL7 FHIR/CDA i openEHR (ISO 13606), a w Polsce P1 i formatu EDM.

| Cecha EHR | Nasza aplikacja |
|---|---|
| Identyfikacja pacjenta i autora | ✅ DID + podpisy |
| Historia wersji (stara wersja zostaje) | ✅ `relatesTo: replaces` + `prev_manifest_cid` |
| Kontrola dostępu i zgody | ✅ UCAN `/ehr/doc/read` + szyfrowanie |
| Integralność i niezaprzeczalność | ✅ CID + podpis OpenPGP |
| Standard wymiany | ✅ FHIR `DocumentReference` |
| Ustrukturyzowane dane kliniczne | ❌ |
| Ślad audytowy (kto co otworzył) | ❌ |
| Słowniki kodów (ICD-10, LOINC) | ❌ |
| Integracja z P1 | ❌ |

**Wniosek:** to jest **zdecentralizowany system przechowywania i udostępniania dokumentacji medycznej z metadanymi FHIR**. To prototyp EHR opartego na dokumentach, a nie pełny EHR. FHIR jest użyty poprawnie w zakresie `DocumentReference`.

## 3. PeerId i jego unikalność

```
pierwsze uruchomienie → Keypair::generate_ed25519() (losowość z CSPRNG systemu)
                      → zapis do keys/node-key.pb (protobuf, chroniony hasłem)
klucz publiczny (32 B) → protobuf → multihash identity → base58 → "12D3KooW…"   = PeerId
ten sam klucz publiczny → multicodec ed25519 → base58btc → "did:key:z6Mk…"     = DID
```

ID pochodzi z biblioteki `libp2p-identity`. Unikalności nie zapewnia żaden rejestr, tylko prawdopodobieństwo. Możliwych kluczy jest 2²⁵⁶, więc szansa, że dwa losowe klucze się powtórzą, wynosi około 2⁻¹²⁸, czyli praktycznie zero. Drugi peer nie może też podszyć się pod cudzy PeerId, bo handshake Noise dowodzi, że ma klucz prywatny.

## 4. Protobuf oraz struct/impl w porównaniu z Javą

**Gdzie używamy protobuf:**
- Wiadomości Bitswap (`ipfs-bitswap/codec.rs`, `models.rs`, przez `prost`)
- Węzły UnixFS dag-pb (`ipfs-unixfs/dagpb.rs`)
- `node-key.pb` (format kluczy libp2p)
- Wewnątrz libp2p: Identify, Kademlia, handshake Noise

Nasze własne protokoły EHR (contact, inbox, replicate) używają **podpisanych ramek JSON** (`wire.rs`), a nie protobuf.

**Struct i impl a Java:**

| Rust | Java | W UML |
|---|---|---|
| `struct` (pola) + `impl` (metody) | klasa | jedna klasa |
| `trait` | interfejs | «interface» |
| `impl Trait for X` | `implements` | realizacja (przerywana strzałka) |
| `enum` | enum, ale warianty mogą nieść dane | «enumeration» |
| `mod` z wolnymi funkcjami | statyczna klasa narzędziowa | «utility» |

Rust nie ma dziedziczenia. `impl EhrNode` jest rozbite na `doctor.rs`, `patient.rs`, `replica.rs` i `open.rs`, ale w UML to **jedna klasa**, podobnie jak klasa częściowa (partial class).

## 5. Slint w skrócie

UI opisuje się deklaratywnie w plikach `.slint`: komponenty, właściwości i callbacki. W czasie budowania `slint-build` kompiluje je do kodu Rust, więc UI nie jest interpretowane podczas działania aplikacji.
- Rust ustawia właściwości (listy, teksty) i podpina callbacki (przycisk → `ehr-core`).
- Slint sam przerysowuje widok, gdy zmieni się właściwość.

W naszej aplikacji `app.slint` to główne okno. Części są w `ui/` (logowanie, pasek boczny, dokumenty, szczegóły, okna dialogowe, widżety), a `src/*.rs` łączy callbacki z `ehr-core`.

## 6. Rozszerzenie pliku, indeks i „nazwa → najnowszy CID”

**Rozszerzenie:**
- Przy wystawianiu lub aktualizacji dokumentu `file_name_with_extension` dopisuje do nazwy rozszerzenie pliku źródłowego, jeśli go brakuje (`"Wynik krwi"` + `scan.pdf` → `"Wynik krwi.pdf"`).
- `media_type` wynika z rozszerzenia.
- Bajty po odszyfrowaniu to **oryginalny plik**, więc po pobraniu wideo zostaje wideo, a tekst tekstem.
- Pobrane pliki trafiają do `~/Downloads/<nazwa>` albo do `(1) <nazwa>`, jeśli plik o tej nazwie już istnieje.

**Indeks.** Działa według `doc_id`, a nie nazwy pliku. Nazwa jest tylko etykietą i może się zmienić przy aktualizacji.

```
/index/{patient_did}/{issuer_did}/{seq u64 BE} → IndexEntry JSON
   seq=1 doc_id=A v1 manifest=bafy..1
   seq=2 doc_id=B v1 manifest=bafy..2
   seq=3 doc_id=A v2 manifest=bafy..3 prev=bafy..1   ← aktualizacja A
lista w aplikacji: grupowanie po (issuer, doc_id) → najwyższa wersja (store.rs:187)
   → A pokazuje v2 (bafy..3), B pokazuje v1; v1 dokumentu A zostaje w historii
inbox: pacjent wysyła since=ostatni seq → lekarz zwraca wpisy seq > since
```

**Prefiksy w LevelDB:**

| Baza | Prefiks | Klucz → wartość | Po co |
|---|---|---|---|
| `ehr-blockstore.leveldb` | `/blocks/` | multihash → bajty | bloki IPFS |
| | `/pins/` | multihash → CID | ochrona przed GC (dokumenty wystawcy, repliki, dokumenty pobrane przez pacjenta) |
| | `/provides/` | multihash → CID | co ogłaszamy w DHT; ponowne ogłoszenie po restarcie; tylko te bloki wydajemy innym |
| | `/remote-providers/` | multihash + peer → rekord | rekordy dostawców z DHT, wygasają po 48 h |
| | `/peers/` | PeerId → adres + seen | ponowne łączenie po restarcie |
| | `/cache/` | — | zarezerwowany, na razie nieużywany (przygotowanie do skalowania) |
| `contacts.leveldb` | `/patient/` | DID → kontakt | pacjenci lekarza |
| | `/doctor/` | DID → kontakt | lekarze pacjenta (zaufani wystawcy) |
| | `/pending/` | DID → kontakt | prośby o kontakt czekające na akceptację lekarza |
| | `/invite/` | kod → zaproszenie | jednorazowe, ważne 1 godzinę |
| `index.leveldb` | `/index/` | patrz wyżej | wersje dokumentów |

**Mechanizmy czyszczenia wdrożone teraz:**
1. **GC pacjenta:** co godzinę (i przy starcie). Gdy bloki zajmują więcej niż 10 GB, usuwany jest każdy nieprzypięty blok (cache) razem z wpisem `/provides/`, a ogłaszanie w DHT zostaje zatrzymane. Własne dokumenty pacjenta są przypięte przy pobraniu, więc GC ich nie usuwa.
2. **Lekarz:** bez limitu rozmiaru. Jego własne dokumenty i repliki są przypięte.
3. **Rekordy dostawców w DHT:** wygasają po 48 h (`PROVIDER_RECORD_TTL_SECS`), w obu rolach.
4. **Zaproszenia:** jednorazowe, wygasają po 1 godzinie.

## 7. Ostatnie zmiany i wnioski (2026-11-25)

### Replikacja: wariant a+

- `/ehr/replicate` obsługuje **tylko program lekarza** (`ehr-core/src/node.rs`). Pacjent nie przyjmuje replik, więc nikt obcy nie zapełni mu dysku przypiętymi blokami.
- Każdy dokument pobrany w aplikacji EHR jest **przypinany** (`EhrNode::fetch`). U pacjenta GC 10 GB usuwa więc tylko cache, a nie jego dokumentację.
- Replika przyjmuje najwyżej **2 CID** (dokument + manifest, `REPLICATE_MAX_CIDS`).
- 3 kopie = lekarz wystawca + pacjent + inny lekarz (węzeł placówki).
- Test: GC z limitem 0 nie usuwa dokumentu pacjenta (`ehr-core-unit-test.rs`). Bez pinowania test się nie udaje.

### Tryb DHT pacjenta zależny od baterii

Dotyczy pacjenta uruchomionego z `--dht-server` (CLI). Lekarz zawsze jest serwerem DHT, a pacjent w aplikacji desktop zawsze klientem.

| Bateria | Tryb |
|---|---|
| poniżej 33% (także przy ładowaniu) | client |
| od 40% albo ładowanie | server |
| 33–39% bez ładowania | bez zmian (histereza, żeby tryb nie skakał) |
| brak baterii albo brak odczytu | zawsze client |

```
co 60 s: /sys/class/power_supply/*/capacity, status
   → patient_dht_server(bateria, obecny tryb)
   → zmiana? NetworkCommand::SetDhtMode → kademlia.set_mode()   (bez restartu)
   → identify push: protokół /ipfs/lan/kad/1.0.0 znika albo wraca
   → inne węzły: DHT_PEER_REMOVED albo ponowne dodanie do tablicy routingu
```

**Sprawdzone w środowisku testowym:** prawdziwe programy `ehr-cli-doctor` i `ehr-cli-patient`. Pacjent działał w osobnej przestrzeni nazw Linuksa (`unshare --user --mount`), z podmontowanym sztucznym katalogiem baterii. Zmieniałem plik `capacity` i `status` w trakcie działania:

| Krok | Pacjent | Tablica routingu lekarza |
|---|---|---|
| start, 80% | Server | jest |
| 20% | Client (po ok. 40 s) | usunięty (`DHT_PEER_REMOVED`) |
| 35% bez ładowania, 70 s | bez zmian (Client) | — |
| 35% z ładowaniem | Server | znowu jest |
| brak baterii | Client | usunięty |
| start od razu przy 15% | `[DHT] LAN Client` | — |

Przy tym teście wyszła usterka w `ipfs-network`: węzeł dodawał peera do tablicy DHT, gdy ten ogłaszał protokół DHT, ale **nie usuwał** go, gdy protokół znikał. Teraz usuwa (`events.rs`). Do tego CLI wypisywało przy starcie tryb z argumentu, a nie rzeczywisty. Teraz wypisuje rzeczywisty.

Ograniczenie: odczyt baterii działa tylko na Linuksie i WSL2. Na innych systemach pacjent zostaje klientem.

### Przejście przez UI: lekarz i pacjent na jednym komputerze

Oba okna w jednym folderze roboczym (`ehr-doctor-data` + `ehr-patient-data`), prawdziwe kliknięcia i wpisywanie, logika `ehr-core`. Przeszło: profile i logowanie hasłem (złe hasło odrzucone), zaproszenie i prośba o kontakt, wystawienie PNG z dopisanym rozszerzeniem, podgląd i weryfikacja podpisu u pacjenta, pobranie do `~/Downloads` (bez nadpisywania), nowe wersje v2 i v3 przez inbox, wylogowanie.

Naprawione usterki:
1. **„Edytuj” zostawiał panel na „Odszyfrowywanie…”.** Edycja zaznaczała dokument, ale nie uruchamiała odszyfrowania. Teraz jedna funkcja `show_preview` (`ehr-desktop/src/documents.rs`) działa przy zaznaczeniu, edycji i po zapisie wersji.
2. **Okno zaproszenia mówiło „ważne 7 dni”**, a zaproszenie jest ważne 1 godzinę. Tekst poprawiony.

### Uwagi praktyczne

- Lekarz i pacjent na jednym komputerze potrzebują różnych portów, np. `EHR_LISTEN_PORT=4311` i `4312`. Inaczej drugi węzeł nie wystartuje.
- Pole „Plik” przyjmuje pełną ścieżkę. Przycisk „Wybierz…” w WSL wymaga `zenity`.
- Model cyklu życia danych z `knowledge/07-data-lifecycle.md` (stany rekordu, usuwanie, crypto-shredding, retencja od końca roku) jest kierunkiem dalszej pracy, a nie stanem kodu.
