W ekosystemie IPFS formaty plików takie jak XML, TTL, BMP, SVG, TIFF czy TXT nie wymagają definiowania nowych, dedykowanych wartości multicodec w pliku consts.rs. IPFS identyfikuje zawartość binarną i tekstową na poziomie bloków używając kodeka CODEC_RAW (0x55) dla surowych danych lub CODEC_DAG_PB (0x70) w przypadku całych grafów UnixFS. Wyjątkiem może być plik hl7-fhir-patient-example.json, który można natywnie obsłużyć zdefiniowanym już CODEC_JSON (0x0200).
Testy integracyjne sprawdzają, jak różne komponenty Twojego systemu (lub biblioteki) współpracują ze sobą jako całość. W odróżnieniu od testów jednostkowych, które badają wyizolowane funkcje "od wewnątrz", testy integracyjne traktują Twój kod jako "czarną skrzynkę".

# Dokumentacja fazy implementacji: docs/phase-2-CID.md

## Moduł IPFS-CID: Identyfikacja treści (Etap M1)

**Typ projektu:** Praca inżynierska
**Cel modułu:** Autorska implementacja Content Identifier (CIDv1) zgodnie ze specyfikacją IPFS, obsługująca wielowarstwowe kodowanie (Multihash, Multicodec, Multibase).
**Status:** Zaimplementowano i przetestowano z Kubo.

---

## 1. Architektura modułu `ipfs-cid`

Zgodnie z zasadą separacji odpowiedzialności, moduł został podzielony na silnie typowane struktury danych. Zamiast operować na surowych ciągach bajtów, wprowadzono oddzielne reprezentacje:

*   **`Multihash`**: Przechowuje kod algorytmu (np. SHA2-256), rozmiar digestu oraz sam wynik funkcji skrótu.
*   **`Cid`**: Główna struktura łącząca wersję (CIDv1), identyfikator formatu (`multicodec`, np. `raw`, `json`) oraz obiekt `Multihash`.
*   **Wydzielone stałe i błędy**: Typowane błędy (oparte na `thiserror`) ułatwiają rygorystyczną walidację narzucaną przez standard (wymagania `MUST`).

---

## 2. Problemy projektowe i techniczne (Q&A z procesu deweloperskiego)

Podczas implementacji napotkano szereg wyzwań specyficznych dla języka Rust oraz środowiska testowego IPFS. Poniższe zagadnienia stanowią materiał analityczny do pracy inżynierskiej.

**Problem 1: Obsługa błędów zewnętrznych bibliotek (Varint)**
*   **Pytanie:** Jak poprawnie obsłużyć błąd `unsigned_varint::decode::u64`, który nie pozwala na użycie operatora `?` ze względu na niezgodność typów konstruktora błędu `CidError::VarintDecodeError(u64)`?
*   **Rozwiązanie:** Błąd wynikał z przypisania typu `u64` do wariantu błędu w enumię. W momencie wystąpienia błędu dekodowania, wartość `u64` nie istnieje. Usunięto argument z wariantu błędu (`VarintDecodeError`) i zastosowano mapowanie błędu domknięciem: `.map_err(|_| CidError::VarintDecodeError)?`. Specyfikacja IPFS wymaga jedynie odrzucenia błędnego varintu, bez konieczności logowania jego technicznej przyczyny.

**Problem 2: Prywatność modułów i import struktur w testach**
*   **Pytanie:** Dlaczego testy w `lib.rs` zgłaszały błąd `unresolved item import Cid is private` po przeniesieniu struktury do pliku `models.rs`, a metody z bloku `impl` nie były wykrywane?
*   **Rozwiązanie:** W Rust bloki `impl` nie są importowane bezpośrednio. Importuje się wyłącznie strukturę (`struct`), a kompilator automatycznie podpina pod nią metody, pod warunkiem, że plik z implementacją jest zadeklarowany (`pub mod cid;`). Rozwiązano problem dodając deklaracje modułów w głównym pliku biblioteki oraz nadając strukturom modyfikator widoczności `pub`.

**Problem 3: Weryfikacja zawartości przez CID w testach integracyjnych**
*   **Pytanie:** Czy w teście integracyjnym można odkodować treść z wygenerowanego CID, aby sprawdzić jej poprawność?
*   **Rozwiązanie:** Nie. CID (bazujący na algorytmach takich jak SHA2-256) jest jednokierunkowym skrótem kryptograficznym. Architektura adresowania treścią zakłada, że CID to tylko "odcisk palca". Walidacja (testowanie) polega na użyciu metody `verify_block(data)`, która hashuje dostarczone z zewnątrz oryginalne dane źródłowe i porównuje wynik z digestem zapisanym w strukturze CID.

**Problem 4: Współbieżność testów Cargo a blokada `repo.lock` w Kubo**
*   **Pytanie:** Dlaczego testy interoperacyjności zgłaszały błąd `someone else has the lock` na pliku `repo.lock` przy próbie wygenerowania CID przez proces Kubo?
*   **Rozwiązanie:** Narzędzie `cargo test` domyślnie uruchamia testy wielowątkowo. Kilka testów jednocześnie próbowało wykonać `ipfs block put` na tym samym lokalnym repozytorium testowym, co powodowało konflikt dostępu do bazy danych. Problem rozwiązano wymuszając sekwencyjne wykonanie testów interoperacyjności za pomocą flagi `--test-threads=1` (lub alternatywnie poprzez atrybut `#[serial]` z biblioteki `serial_test`).

**Problem 5: Optymalizacja struktury testów czarnoskrzynkowych w Workspace**
*   **Pytanie:** Jak oddzielić wewnętrzne testy jednostkowe od testów integracyjnych i współdzielić pliki atrap (mocks) bez łamania konwencji Cargo?
*   **Rozwiązanie:** Zastosowano dedykowany pakiet `test-runner`. Testy jednostkowe pozostały wewnątrz `crates/ipfs-cid`, natomiast testy integracyjne i interoperacyjności przeniesiono do `tests/test-runner/tests/`. Zapewniło to czystą architekturę, w której `test-runner` importuje `ipfs-cid` z zewnątrz (jak docelowy użytkownik), a ścieżki do współdzielonego folderu `mocks/` uległy znacznemu uproszczeniu.

---

## 3. Logika testów i przypadki brzegowe (Edge Cases)

Implementacja została pokryta trzema warstwami testów, gwarantującymi bezpieczeństwo i zgodność ze standardem IPFS.

### Testy jednostkowe (Unit Tests)
Badają wyizolowane metody, zachowanie struktur i obsługę protokołów.
*   **Edge Case - Trailing Bytes:** Odrzucanie pakietów binarnych, które po poprawnym odkodowaniu `Multihash` zawierają dodatkowe, nieoczekiwane bajty na końcu (wymóg ścisłego dekodowania `MUST`).
*   **Walidacja wersji:** Odrzucanie identyfikatorów z ustawioną wersją inną niż `CIDv1`.
*   **Błędy parsowania:** Sprawdzanie, czy funkcja prawidłowo rzuca błąd przy braku poprawnego prefiksu `Multibase` w reprezentacji tekstowej.
*   **Weryfikacja kryptograficzna:** Potwierdzenie poprawnego generowania prefiksów bajtowych dla algorytmów SHA2-256 oraz SHA2-512.

### Testy integracyjne (Integration Tests)
Traktują moduł jako "czarną skrzynkę", testując współpracę komponentów.
*   **Pełny cykl życia (Lifecycle):** Generowanie CID z danych -> serializacja do tablicy bajtów -> kodowanie do Base32 -> odkodowanie z tekstu -> odkodowanie z bajtów -> weryfikacja zgodności zreferowanego bloku.
*   **Edge Case - Integralność danych:** Próba pozytywnej weryfikacji bloku (`verify_block`), w którym sztucznie zmieniono jeden bajt danych. Metoda musi zwrócić `false`.
*   **Format Agnosticism:** Udowodnienie, że reprezentacja tekstowa nie wpływa na reprezentację binarną. Serializacja tego samego identyfikatora do formatu Base32 (prefiks `b`) oraz Base58btc (prefiks `z`), a następnie wykazanie, że obie odkodowują się do identycznego, matematycznego obiektu CID.

### Testy interoperacyjności (Interoperability Tests)
Uruchamiają zewnętrzny proces demona Kubo, aby udowodnić faktyczną kompatybilność.
*   **Testowanie wieloformatowe:** Wykorzystano atrapy z folderu `mocks`. Moduł w Rust generuje CID dla plików w formatach XML, TTL, BMP, SVG, TIFF, TXT (traktowanych przez IPFS jako `CODEC_RAW`) oraz plików JSON (obsługiwanych natywnie jako `CODEC_JSON`).
*   **Weryfikacja CLI:** Systematyczne porównywanie wyniku tekstowego z kodem Rust z wyjściem systemowej komendy: `ipfs block put --cid-codec=<codec> --mhtype=sha2-256 --offline <file_path>`. Wszystkie testy wykazują 100% zgodności znaków.
