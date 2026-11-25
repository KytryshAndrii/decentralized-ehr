# Propozycja integracji UCAN + GPG jako osobnej warstwy (role: lekarz / placówka i pacjent)

> **Stan:** 16.11.2026. To jest **propozycja projektowa, nie opis kodu**. Nic z tego dokumentu nie jest jeszcze zaimplementowane ani skompilowane. Wersje bibliotek pochodzą z rozeznania w sieci z 16.11.2026 i **nie były sprawdzone kompilacją** z `libp2p 0.57.1`. Miejsca oznaczone *(do weryfikacji)* wymagają potwierdzenia przed implementacją.
>
> Dokument rozwija analizę z `docs/phase-1.1-planning-role-models.md` (wariant C: IPFS + UCAN + GPG) i pokazuje, **jak wpiąć oba mechanizmy w istniejący kod bez przepisywania crate'ów `ipfs-*`**.

## 1. Zasada: warstwa nad IPFS, nie w IPFS

```text
ehr-cli  (main.rs → services::SecurityService, EhrService)          NOWE serwisy
   │
ehr-security  (nowy crate: tożsamość, UCAN, OpenPGP, rejestr zaufania)   NOWY
   │  szyfrogram + manifest (zwykłe bajty)
ehr-core      (M9: manifest, rejestr pacjent ↔ dokument ↔ CID)
   │
ipfs-unixfs → ipfs-store → ipfs-bitswap / ipfs-network              BEZ ZMIAN
```

- Warstwa bezpieczeństwa **produkuje i konsumuje zwykłe bajty**. Dla IPFS szyfrogram jest takim samym plikiem jak każdy inny: `ipfs_unixfs::add_file`, `cat`, `provide`, `fetch` działają bez zmian.
- **Szyfrowanie przed haszowaniem** (encrypt-then-hash): CID jest liczony z szyfrogramu. Blockstore i Bitswap nadal weryfikują integralność, a węzeł przechowujący blok nie potrzebuje żadnych kluczy.
- Zależność jest jednokierunkowa: `ehr-security → ipfs-cid` (CID w tokenach) i klucz z `ipfs_network::identity`. Żaden crate `ipfs-*` nie zależy od `ehr-security`.

## 2. Podział odpowiedzialności

| Pytanie | Mechanizm | Gdzie sprawdzane |
|---|---|---|
| Kto może **odczytać treść**? | OpenPGP: szyfr hybrydowy, klucz sesji opakowany dla każdego odbiorcy | przy odszyfrowaniu, lokalnie u odbiorcy |
| Kto **wystawił** dokument? | OpenPGP: podpis wystawcy wewnątrz szyfrogramu | po odszyfrowaniu |
| Czy wystawca **miał prawo** wystawić dokument medyczny? | UCAN: delegacja `/ehr/doc/issue` od zaufanej placówki | przy przyjęciu dokumentu |
| Czy pacjent **ma prawo** odczytać dokument? | UCAN: delegacja `/ehr/doc/read` **od lekarza** (nigdy odwrotnie), z terminem ważności | przy wystawieniu dokumentu (opakowanie klucza dla pacjenta) |
| Czy bajty są **nienaruszone**? | CID (już jest) | Blockstore, Bitswap |

Sam UCAN nie chroni poufności (kto zna CID, pobierze blok), a sam podpis GPG nie mówi, że podpisujący jest lekarzem. Dlatego potrzebne są oba.

## 3. Tożsamość: jeden klucz węzła = podmiot UCAN

- Węzeł ma już klucz Ed25519 w `./tmp/node-key.pb` (`ipfs_network::identity::load_or_create`). Z tego samego klucza publicznego powstaje **`did:key`**:
  `did:key:z` + base58btc(`0xed 0x01` + 32 bajty klucza publicznego) → `did:key:z6Mk...`.
- Mapowanie PeerId ↔ did:key jest odwracalne (PeerId Ed25519 zawiera klucz publiczny), więc **tożsamość sieciowa i tożsamość autoryzacyjna to ten sam podmiot**. Tokeny UCAN podpisuje klucz węzła (`Keypair::sign`), weryfikuje `PublicKey::verify` z `libp2p-identity`. Nie trzeba nowej biblioteki kryptograficznej.
- **Klucz OpenPGP jest osobny** (podpis + podklucz szyfrujący), bo Ed25519 węzła nie szyfruje. Powiązanie robi krótki rekord „ten odcisk klucza OpenPGP należy do tego DID”, podpisany kluczem węzła. Rekord jest zwykłym blokiem w IPFS i trafia do lokalnego rejestru zaufania.

Potrzebne są dwie małe funkcje (`did_from_keypair`, `public_key_from_did`) w nowym crate; `ipfs-network` się nie zmienia.

## 4. Role bez serwera centralnego

> **Uproszczenie przyjęte w pracy:** placówka i lekarz to **jeden węzeł** (jeden klucz, jeden adres). Korzeń łańcucha `/ehr/doc/issue` to klucz tego węzła (token wystawiony samemu sobie: `iss = sub = DID węzła lekarza`). Pacjent dodaje DID lekarza do lokalnej listy zaufanych przy imporcie wizytówki lekarza (`contact add`). Wiersze „Placówka” i „Lekarz” w tabeli poniżej opisują więc ten sam węzeł; rozdzielenie na dwa węzły jest rozszerzeniem poza zakres. Szczegóły i kolejność prac: `docs/phase-9-ehr-delivery-plan.md` (nadrzędny).

Ten sam program, różne uprawnienia (zgodnie z regułą projektu). Rola wynika z **łańcucha delegacji**, a nie z flagi w konfiguracji:

| Podmiot | Skąd bierze uprawnienie | Co może |
|---|---|---|
| **Placówka** | jej DID jest w lokalnej liście zaufanych wystawców (`./data/trusted-issuers`), dodanej ręcznie przez użytkownika | korzeń zaufania dla `/ehr/doc/issue` |
| **Lekarz** | delegacja UCAN od placówki: `cmd = /ehr/doc/issue` | wystawia dokumenty (upload), tworzy nowe wersje (zmiana), czyta i pobiera; **jako jedyny** nadaje pacjentowi `/ehr/doc/read` |
| **Pacjent** | delegacja UCAN od lekarza: `cmd = /ehr/doc/read`, `exp`; jest podmiotem (`sub`) dokumentacji | **tylko odczyt i pobranie**; nie wystawia, nie zmienia, nie udostępnia |

Dzięki temu pacjent nie podszyje się pod lekarza: może podpisać plik własnym kluczem, ale nie ma łańcucha `/ehr/doc/issue` zaczynającego się u zaufanej placówki, więc aplikacja oznaczy plik jako „prywatny plik pacjenta”, a nie „dokument medyczny” (scenariusz T08).

**Reguła „pacjent nie deleguje”:** weryfikator przyjmuje token `/ehr/doc/read` tylko wtedy, gdy jego `iss` ma własny łańcuch `/ehr/doc/issue` do zaufanej placówki. Token podpisany przez pacjenta (np. dla innego lekarza) nie ma takiego łańcucha, więc jest odrzucany. Lekarz nadaje dostęp innemu lekarzowi tym samym mechanizmem. Zniesiono polecenie `/ehr/doc/share`.

### 4.1. Token (podzbiór UCAN 1.0)

Pola jak w UCAN 1.0 Delegation: `iss`, `aud`, `sub`, `cmd`, `pol`, `nonce`, `exp`, `nbf`. Czas w **sekundach Unix UTC**. Proponowany zakres dla pracy:

- polecenia: `/ehr/doc/issue` (lekarz: upload, nowa wersja), `/ehr/doc/read` (odczyt i pobranie; pacjent i lekarz);
- `pol` ograniczone do jednego warunku: lista CID dokumentów albo „wszystkie dokumenty podmiotu”;
- token jest blokiem w IPFS, a dowód (`prf`) to lista CID tokenów.

Weryfikator sprawdza kolejno: podpis każdego ogniwa kluczem z `iss`; `nbf`/`exp` każdego ogniwa; ciągłość łańcucha (`aud` ogniwa = `iss` następnego, korzeń wystawiony przez `sub` albo przez zaufaną placówkę); czy polecenie jest równe lub węższe niż w dowodzie; czy CID dokumentu spełnia `pol`; czy żadne ogniwo nie jest na lokalnej liście unieważnień.

### 4.2. Biblioteka czy własny weryfikator

| Opcja | Stan (16.11.2026, *do weryfikacji*) | Ocena |
|---|---|---|
| crate `ucan` 0.4.0 | ostatnie wydanie 2023, format JWT sprzed UCAN 1.0 | przestarzały |
| `ucan-wg/rs-ucan` (gałąź główna) | UCAN 1.0-rc.1, „work in progress”, brak wydania na crates.io | tylko zależność z gita, bez audytu |
| `ssi-ucan` | utrzymywany, ale wciąga duży stos `ssi` | za ciężki |
| **własny, minimalny weryfikator** | kilkaset linii na `libp2p-identity` + `serde` | **rekomendacja** |

Rekomendacja: własny weryfikator **formatu i łańcucha** (nie kryptografii; podpisy robi `libp2p-identity`), z nazwami pól UCAN 1.0. W pracy trzeba jasno napisać, że to **podzbiór, bez gwarancji interoperacyjności** z innymi implementacjami UCAN. Powód: w Rust nie ma dziś wydanej, aktualnej biblioteki UCAN, a zależność z gita w stanie „work in progress” jest większym ryzykiem niż 300 linii z testami.

## 5. Szyfrowanie i podpis: OpenPGP

| | `pgp` (rPGP) | `sequoia-openpgp` | binarka `gpg` |
|---|---|---|---|
| Licencja | MIT / Apache-2.0 | LGPL-2.0+ | GPL (osobny proces) |
| Zależności systemowe | brak (czysty Rust) | domyślnie biblioteka C `nettle` | zainstalowany GnuPG, keyring, agent |
| Wielu odbiorców | `MessageBuilder` + `encrypt_to_key` dla każdego | tak | `-r A -r B` |
| Testy automatyczne | proste | proste | kruche (stan keyringu, parsowanie wyjścia) |

**Rekomendacja: rPGP** (`pgp`, wersja 0.21.x *do weryfikacji*; wymaga Rust ≥ 1.88, w projekcie jest 1.98.1). Czysty Rust, bez bibliotek systemowych na trzech maszynach demonstracyjnych, a wynik da się odszyfrować zwykłym `gpg`, co pokazuje „zgodność z GPG”. Dla zgodności z GnuPG: klucze v4 i pakiet SEIPD v1 *(do weryfikacji)*.

Model szyfrowania jest hybrydowy i OpenPGP robi go natywnie: jeden losowy klucz sesji szyfruje treść, a dla każdego odbiorcy powstaje osobny pakiet z tym kluczem opakowanym jego kluczem publicznym.

## 6. Przepływy

### 6.0. Rejestracja pacjenta (jednorazowo): skąd lekarz zna DID pacjenta

`provide` w IPFS **nie jest adresowane**: ogłasza w DHT „mam CID X”, a pobiera ten, kto zna CID. Lekarz nie wskazuje więc PeerId pacjenta, żeby coś „dostarczyć”. Dane pacjenta są mu potrzebne do trzech innych rzeczy: zaszyfrowania dokumentu (klucz publiczny OpenPGP), wpisania `aud` w tokenie UCAN (DID) i weryfikacji podpisu żądania skrzynki (klucz publiczny jest w DID).

1. Pacjent uruchamia `ehr-cli identity contact`: wizytówka `{ did, klucz publiczny OpenPGP, podpisane kluczem węzła powiązanie „ten odcisk należy do tego DID”, opcjonalnie multiaddr }` jako tekst albo kod QR.
2. Przy wizycie w placówce lekarz importuje wizytówkę: `ehr-cli contact add <wizytówka>` (lokalna książka kontaktów `./data/contacts`). Weryfikacja twarzą w twarz zastępuje urząd certyfikacji.
3. **PeerId wynika z DID** (klucz Ed25519 jest w obu), więc adresu pacjenta można szukać w DHT po PeerId, a wizytówka może go podać jako wskazówkę.

### 6.1. Lekarz wystawia dokument pacjentowi

```text
plik → podpis OpenPGP lekarza → szyfrowanie dla {pacjent, placówka}
     → szyfrogram → ipfs_unixfs::add_file → CID_doc
manifest { doc: CID_doc, sub: DID pacjenta, iss: DID lekarza,
           typ, utworzono_utc, prf: [CID delegacji /ehr/doc/issue] }
     → blok → CID_manifest → provide
```

### 6.2. Pacjent odbiera

`fetch CID_manifest` → `fetch CID_doc` → odszyfrowanie własnym kluczem → weryfikacja podpisu → weryfikacja łańcucha `prf` do zaufanej placówki → status „dokument medyczny” albo „niezweryfikowany”.

### 6.3. Zmiana dokumentu i dostęp innego lekarza (tylko lekarz)

Pacjent **nie udostępnia** dokumentów. Zmiany i nowe uprawnienia wprowadza wyłącznie lekarz:

1. **Zmiana:** bloki w IPFS są niezmienne, więc „zmiana” to nowa wersja. Lekarz podpisuje i szyfruje plik od nowa → nowy CID; nowy manifest wskazuje poprzedni (`prev`). Pacjent zobaczy nową wersję przy kolejnym `inbox sync` (6.4).
2. **Dostęp innego lekarza:** poza zakresem pracy (jeden węzeł lekarza, decyzja D05). Poniższy opis zostaje jako rozszerzenie: lekarz A wystawia lekarzowi B delegację `/ehr/doc/read` i szyfruje dokument ponownie dla {pacjent, A, B} → nowy CID.

Ponowne szyfrowanie zamiast dopisania opakowanego klucza: jest proste, a dopisanie pakietu do istniejącego szyfrogramu i tak zmieniłoby CID. Wariant z kluczem w osobnym małym bloku („koperta klucza”), przy którym CID danych się nie zmienia, zostaje jako możliwe rozszerzenie (decyzja D05).

**Kopia pliku u innej osoby.** Blok lub plik pacjenta może leżeć na urządzeniu innego pacjenta (węzeł przechowuje i ogłasza bloki, Bitswap wydaje je każdemu). To akceptowane: ma tam wyłącznie szyfrogram. Bez klucza prywatnego odbiorcy nie odszyfruje go (OpenPGP), a podpis lekarza uniemożliwia podmianę treści bez wykrycia (podpis + CID).

### 6.4. Jak pacjent dowiaduje się o dokumencie (pobieranie od znanego węzła)

Lekarz **nie łączy się z pacjentem**: nie zna jego adresu, a pacjent może być wyłączony. Pacjent zna adres węzła lekarza z wizytówki (6.0) i sam do niego dzwoni.

1. Przy wystawieniu dokumentu lekarz dopisuje wpis do swojego indeksu (nazwa pliku, `CID_manifest`, numer kolejny) pod DID pacjenta.
2. Pacjent uruchamia `inbox sync` (przy starcie aplikacji albo co N sekund). Węzeł łączy się z lekarzem (adres z wizytówki) i przez `/ehr/inbox/1.0.0` pyta: „co masz dla DID X?”. Zapytanie jest **podpisane kluczem pacjenta**, więc lekarz odpowiada tylko właścicielowi DID.
3. Lekarz odpowiada podpisaną listą `[(numer, CID_manifest, data)]`. Pacjent weryfikuje podpis i łańcuch UCAN, pobiera manifest (`fetch`), odszyfrowuje go i zapisuje w swoim indeksie nazwę pliku. Dopiero wtedy pobiera dokument.
4. **Hook** po zweryfikowanym wpisie: komunikat `[INBOX] NEW "<nazwa pliku>" verified`.

Pacjent nie zna CID i nie musi: dostaje go w kroku 3 i widzi tylko nazwę pliku. Lekarz nie zna adresu pacjenta i nie musi.

**Imię i nazwisko** nigdy nie trafiają do IPFS ani do manifestu. Lekarz widzi je z lokalnej książki kontaktów (`DID -> imię, nazwisko`, wypełnionej przy rejestracji na podstawie dokumentu tożsamości okazanego osobiście). Nazwa pliku jest wewnątrz zaszyfrowanego manifestu.

Ograniczenia: pobranie działa tylko, gdy węzeł lekarza jest włączony i osiągalny; przy wielu węzłach można dodać marker w DHT (`hash("ehr-inbox" + DID)` ogłaszany przez `provide`) jako wariant poza zakresem pracy.

### 6.5. Cały przepływ: od wystawienia do odczytu

```text
LEKARZ (węzeł D, także placówka)                         PACJENT (węzeł P)
────────────────────────────────                         ─────────────────
[jednorazowo] contact add <wizytówka pacjenta>  <──────  identity contact (DID, klucz PGP)
              wizytówka lekarza (adres D)       ──────>  contact add (adres D = bootstrap, DID D zaufany)

doc issue badanie.pdf --patient <DID P> --name "..."
 1. podpis OpenPGP lekarza + szyfrowanie dla {P, lekarz} -> szyfrogram
 2. add_file (stały chunker)               -> CID_doc
 3. token UCAN /ehr/doc/read, aud = DID P  -> CID_ucan
 4. manifest (zaszyfrowany) {nazwa, doc, ...} -> CID_manifest
 5. provide: bloki dokumentu, tokenu, manifestu
 6. wpis w indeksie lekarza
                                                                7. inbox sync: połączenie P -> D (adres z wizytówki)
                                                                   żądanie podpisane przez P, odpowiedź podpisana przez D
                                                                8. weryfikacja podpisu, tokenu (aud = P, exp), łańcucha issue
                                                                9. fetch CID_manifest, odszyfrowanie, wpis w indeksie P
                                                               10. doc open "<nazwa>": fetch CID_doc (Bitswap, fale po 64 bloki)
                                                               11. odszyfrowanie kluczem prywatnym P, weryfikacja podpisu D
                                                               12. odczyt / pobranie (tylko tyle)
```

Zależność praktyczna: pobranie działa, dopóki węzeł lekarza jest włączony (jeden węzeł, brak kopii zapasowej). Pacjent po pobraniu może ogłaszać własną kopię (szyfrogram, który i tak odczyta tylko odbiorca).

## 7. Co dokładnie dochodzi w kodzie

| Miejsce | Zmiana | Istniejący kod |
|---|---|---|
| `crates/ehr-security` (nowy) | `identity.rs` (did:key), `ucan.rs` (token, łańcuch), `pgp.rs` (klucze, szyfrowanie, podpis), `trust.rs` (zaufani wystawcy, powiązania DID ↔ OpenPGP, unieważnienia), `models.rs`, `consts.rs`, `error.rs` | bez zmian |
| `crates/ehr-core` (M9) | manifest dokumentu z polami `iss`, `sub`, `prf` | pusty szablon |
| `apps/ehr-cli/src/services/security_service.rs` (nowy) | serwis łączący `ehr-security` z `IpfsService` | `IpfsService`, `NetworkService` bez zmian |
| `apps/ehr-cli/src/main.rs` | nowe podkomendy (niżej) | istniejące komendy bez zmian |
| `ipfs-store` | bez zmian; nowe dane to zwykłe bloki i pliki w `./data` | — |
| `ipfs-bitswap`, `ipfs-network`, `ipfs-unixfs`, `ipfs-cid` | **bez zmian** | — |

Proponowane komendy:

```text
ehr-cli identity did                              # DID węzła
ehr-cli identity contact                          # wizytówka: DID + klucz publiczny OpenPGP (tekst / QR)
ehr-cli contact add <wizytówka>                   # książka kontaktów lekarza
ehr-cli key generate | key export | key import <plik>
ehr-cli trust add-issuer <DID>                    # placówka jako korzeń zaufania
ehr-cli ucan delegate <DID-odbiorcy> <polecenie> [--doc <CID>] [--exp <sekundy-UTC>]
ehr-cli doc issue <plik> --patient <DID>          # podpis + szyfrowanie + add + manifest
ehr-cli doc open <CID-manifestu> <plik-wyjściowy> # weryfikacja + odszyfrowanie
ehr-cli doc update <CID-manifestu> <plik>         # tylko lekarz: nowa wersja (prev = stary manifest)
```

### 7.1. Jedyne miejsce, gdzie warstwa mogłaby wejść głębiej (opcjonalne)

Serwer Bitswap wydaje dziś ogłoszone bloki każdemu połączonemu peerowi. Kontrola „komu wydać blok” na poziomie Bitswap wymagałaby przekazania `PeerId` do `BlockSource` (dziś trait go nie dostaje), czyli zmiany sygnatury w `ipfs-bitswap`. **Nie jest to potrzebne do poufności**: bloki są szyfrogramem, a scenariusz T05 („pacjent B pobiera szyfrogram pacjenta A”) może się udać z założenia. Zostaje jako utwardzenie na później (ukrywa, że dokument istnieje), a nie jako część tej warstwy.

## 8. Skutki dla UnixFS i deduplikacji

- Losowy klucz sesji oznacza, że ten sam dokument zaszyfrowany dwa razy daje **różne szyfrogramy i różne CID**. Deduplikacja bloków między wersjami zaszyfrowanego dokumentu nie działa.
- **Decyzja: rezygnujemy z chunkera Rabina, zostaje wyłącznie stały chunker (`size-262144`).**
  <!-- Dlaczego: (1) w tym systemie każdy dokument jest szyfrogramem, a Rabin dzieli po treści i
  daje korzyść (reużycie bloków po wstawce) tylko dla jawnych danych z powtarzalnymi fragmentami;
  szyfrogram z losowym kluczem sesji jest pseudolosowy, więc granice Rabina nie "odnajdują się"
  i każdy blok jest inny. (2) Rabin kosztuje czas procesora (100 MB: 9,0 s zamiast 5,3 s przy
  stałym) i ok. 3x więcej bloków (1231 zamiast 386 dla 100 MB), a liczba bloków to koszt `provide`,
  `fetch` i miejsca w bazie. (3) Jeden chunker oznacza mniej kodu i brak wyboru po rozszerzeniu
  (`Chunker::for_path` błędnie traktuje przemianowane pliki). (4) Deduplikacja po szyfrowaniu
  jest niemożliwa z założenia (sekcja poniżej: szyfrowanie zbieżne jest niedopuszczalne w EHR). -->
  Zakres zmiany (jeszcze niewykonanej w kodzie): usunięcie `Chunker::Rabin`, `Chunker::for_path`, tablic Rabina i stałych `RABIN_*` z `ipfs-unixfs`, a z testów i skryptów: wzorce CID Rabina w `unixfs-unit-test.rs` oraz scenariusz C (`--chunker=rabin`) w `test-kubo-unixfs-interop.sh`. Zgodność z Kubo zostaje dla `size-262144`. Pliki z Kubo wykonane Rabinem nadal da się **czytać** (`cat` nie zależy od chunkera), tylko Rust już ich tak nie tworzy.
- Szyfrowanie zbieżne (klucz wyliczany z treści) przywróciłoby deduplikację, ale ujawnia, że dwie osoby mają ten sam dokument. W EHR jest to niedopuszczalne.

## 9. Testy

Scenariusze T01–T10 z `phase-1.1` przechodzą bez zmian. Dochodzą testy UCAN:

| ID | Scenariusz | Oczekiwany wynik |
|---|---|---|
| U01 | delegacja po terminie `exp` | odrzucona |
| U02 | token dla innego odbiorcy (`aud`) | odrzucony |
| U03 | polecenie spoza delegacji (`read` użyte jako `issue`) | odrzucone |
| U04 | przerwany łańcuch (`aud` ≠ `iss` następnego ogniwa) | odrzucony |
| U05 | korzeń łańcucha `issue` spoza listy zaufanych placówek | dokument „niezweryfikowany” |
| U06 | podpis tokenu zmieniony o jeden bajt | odrzucony |
| U07 | delegacja na liście unieważnień | odrzucona |
| U08 | dokument podpisany przez pacjenta, bez łańcucha `issue` | „prywatny plik pacjenta” (T08) |
| U09 | pacjent wystawia `/ehr/doc/read` dla innego lekarza | odrzucone (`iss` bez łańcucha `issue`) |
| U10 | pacjent próbuje wystawić nową wersję dokumentu | odrzucone |
| U11 | odpowiedź skrzynki bez ważnego łańcucha `issue` | odrzucona, wpis nie trafia do indeksu |

Warstwy testów jak w projekcie: jednostkowe w `crates/ehr-security/tests/`, integracyjne na procesach `ehr-cli` (placówka + 2 pacjentów), a interoperacyjność: szyfrogram z aplikacji odszyfrowany przez `gpg --decrypt` i odwrotnie.

## 10. Ograniczenia, które trzeba opisać w pracy

- Cofnięcie delegacji nie odbiera już pobranych i odszyfrowanych kopii; skuteczne jest tylko ponowne szyfrowanie przyszłych wersji.
- Unieważnienia są lokalną listą; bez wspólnego kanału inne węzły dowiedzą się o nich dopiero po jej wymianie.
- Utrata klucza prywatnego OpenPGP oznacza utratę dostępu do dokumentów; potrzebna kopia zapasowa klucza.
- Zaufanie do placówki ustanawia użytkownik ręcznie (dodanie DID). Nie ma urzędu certyfikacji.
- Manifest i rekord dostawcy w DHT ujawniają metadane (kto ma jaki CID). W manifeście nie może być nazwisk, PESEL ani rozpoznań w postaci jawnej.
- Klucz węzła leży niezaszyfrowany w `./tmp/node-key.pb` (uprawnienia `0600`); po tej zmianie staje się też kluczem autoryzacyjnym.

## 11. Kolejność prac (propozycja)

1. `ehr-security::identity` + `ehr-cli identity did` (mały, sprawdzalny krok).
2. OpenPGP: klucze, szyfrowanie dla wielu odbiorców, podpis; testy T01–T10.
3. UCAN: token, weryfikator łańcucha, lista zaufanych placówek; testy U01–U08.
4. Manifest w `ehr-core` (M9) i komendy `doc issue / open / share`.
5. Demonstracja na trzech maszynach (M10).

## 12. Decyzje (rozstrzygnięte dla pracy dyplomowej)

- **D01** klucze OpenPGP generuje aplikacja (`ehr-<doctor|patient>-data/keys/`, tryb `0600`); import z GnuPG poza zakresem.
- **D02** zakres delegacji `/ehr/doc/read`: jeden dokument (`pol` = CID dokumentu), wystawiany przez lekarza.
- **D04** dokumenty podpisuje lekarz; pacjent niczego nie podpisuje poza żądaniem skrzynki.
- **D05** udostępnianie innemu lekarzowi: **poza zakresem** (jeden węzeł lekarza). Komendy `doc grant` nie ma.
- Format tokenu i wiadomości: JSON (`serde_json`), podpis liczony nad dokładnymi bajtami przesyłanego ładunku (bez kanonikalizacji). DAG-CBOR odrzucony: wymaga kodera, którego nie ma w projekcie.

## 13. Źródła

- UCAN: <https://github.com/ucan-wg/spec>, <https://github.com/ucan-wg/delegation>, <https://github.com/ucan-wg/invocation>, <https://github.com/ucan-wg/revocation>
- did:key: <https://w3c-ccg.github.io/did-key-spec>; PeerId: <https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md>
- Rust UCAN: <https://crates.io/crates/ucan>, <https://github.com/ucan-wg/rs-ucan>, <https://crates.io/crates/ssi-ucan>
- OpenPGP: RFC 9580; <https://github.com/rpgp/rpgp>, <https://docs.rs/pgp/latest/pgp/composed/struct.MessageBuilder.html>, <https://docs.rs/crate/sequoia-openpgp/latest/features>
- Wcześniejsze zastosowania UCAN z IPFS: <https://docs.storacha.network/concepts/ucan>, <https://fission.codes/blog/a-guide-to-ucans>
