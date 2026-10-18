# Plan implementacji IPFS w Rust

## Zdecentralizowany system EHR oparty na protokole IPFS

**Typ projektu:** Praca inżynierska  
**Język implementacji:** Rust  
**Charakter projektu:** Analityczno-projektowy, demonstracyjny  
**Architektura:** Rozproszona sieć P2P oparta na IPFS  
**Środowisko demonstracyjne:** 3 fizyczne komputery  
**Implementacja referencyjna:** Kubo (IPFS)  
**Status:** Planowanie architektury i implementacji

---

## 1. Cel projektu

Celem pracy jest analiza możliwości zastosowania protokołu IPFS w systemach elektronicznej dokumentacji medycznej (EHR) oraz przygotowanie demonstracyjnej implementacji zdecentralizowanego rozwiązania umożliwiającego zapis, identyfikację, pobieranie i udostępnianie przykładowych dokumentów medycznych.

Projekt zakłada stworzenie własnej, minimalnej implementacji wybranych mechanizmów IPFS w języku Rust, a następnie wykorzystanie jej jako podstawy do budowy uproszczonego systemu EHR.

Implementacja powinna umożliwiać:

- Uruchamianie niezależnych węzłów IPFS na różnych urządzeniach.
- Dynamiczne dołączanie nowych węzłów do istniejącej sieci.
- Odłączanie i ponowne podłączanie węzłów.
- Identyfikowanie zawartości dokumentów za pomocą CID.
- Lokalny zapis i odczyt bloków danych.
- Wyszukiwanie dostawców treści za pomocą Kademlia DHT.
- Wymianę bloków między węzłami przez Bitswap.
- Obsługę plików medycznych przy wykorzystaniu podstawowych mechanizmów UnixFS.
- Replikację wybranych dokumentów między węzłami.
- Zarządzanie rejestrem dokumentów przypisanych do pacjentów.
- Weryfikację zgodności wybranych mechanizmów z istniejącą implementacją IPFS — Kubo.

Projekt nie zakłada stworzenia pełnego systemu produkcyjnego ani nowego protokołu sieciowego.

Główny nacisk zostanie położony na działanie systemu rozproszonego, komunikację P2P, identyfikację danych, ich dostępność oraz interoperacyjność.

---

## 2. Główne decyzje architektoniczne i ich uzasadnienie

Poniższe decyzje wynikają z przyjętego zakresu pracy oraz wcześniejszej analizy możliwości implementacji IPFS.

### AD-01. Implementacja w języku Rust

- Głównym językiem programowania będzie Rust.
- Logika EHR oraz wybrane mechanizmy IPFS zostaną zaimplementowane w Rust.
- Wszystkie trzy komputery będą uruchamiały tę samą aplikację, z odmienną konfiguracją ról.

**Uzasadnienie:**

Rust umożliwia programowanie systemowe i sieciowe, zapewnia kontrolę nad pamięcią i zasobami oraz posiada biblioteki do komunikacji asynchronicznej i P2P.

Umożliwia implementację mechanizmów sieciowych bez konieczności stosowania wielu języków programowania.

### AD-02. Własna minimalna implementacja IPFS zamiast zależności od Kubo

- Implementowane będą wybrane mechanizmy IPFS.
- Docelowa aplikacja nie będzie wymagać uruchomionego procesu Kubo.
- Kubo zostanie wykorzystane jako implementacja referencyjna do testowania.

**Uzasadnienie:**

Celem pracy jest wykazanie umiejętności projektowania i implementowania systemów rozproszonych, a nie wyłącznie stworzenie aplikacji korzystającej z gotowego serwera IPFS przez API.

Jednocześnie nie ma potrzeby implementowania całego protokołu IPFS od zera.

Przyjęte podejście pozwoli wykorzystać istniejące biblioteki realizujące standardowe mechanizmy transportowe, jednocześnie samodzielnie implementując wybrane komponenty wyższego poziomu.

### AD-03. Wykorzystanie rust-libp2p

- Komunikacja pomiędzy węzłami będzie realizowana za pomocą `rust-libp2p`.
- Wykorzystane zostaną istniejące mechanizmy TCP, Noise, Yamux, Identify i Ping.
- Do realizacji Kademlia DHT planowane jest wykorzystanie modułu `libp2p::kad`.

**Uzasadnienie:**

Samodzielne implementowanie całego stosu transportowego znacząco zwiększyłoby zakres pracy.

Biblioteka libp2p pozwala skoncentrować się na integracji protokołów IPFS i logice działania systemu.

Wykorzystanie biblioteki nie oznacza stworzenia własnego protokołu, ponieważ opieramy się na istniejących specyfikacjach.

### AD-04. Trzy niezależne węzły IPFS

Środowisko demonstracyjne będzie składać się z trzech komputerów:

- **Węzeł A — placówka medyczna:** tworzy i udostępnia dokumenty.
- **Węzeł B — pacjent 1:** zarządza swoim rejestrem CID, pobiera i przechowuje dokumenty.
- **Węzeł C — pacjent 2:** posiada analogiczne możliwości jak węzeł B.

Każde urządzenie będzie pełnoprawnym uczestnikiem sieci P2P w zakresie funkcji obsługiwanych przez naszą implementację.

**Uzasadnienie:**

Trzy fizyczne urządzenia umożliwiają demonstrację rzeczywistej komunikacji rozproszonej, odkrywania węzłów, replikacji danych oraz zachowania sieci przy odłączeniu uczestnika.

Architektura nie wymaga centralnego serwera do przechowywania wszystkich dokumentów.

### AD-05. Dynamiczne dołączanie i odłączanie węzłów

- Każdy węzeł posiada trwałą tożsamość Peer ID.
- Nowe urządzenie może dołączyć do działającej sieci.
- Utrata pojedynczego węzła nie powinna zatrzymywać komunikacji między pozostałymi uczestnikami.
- Ponowne uruchomienie węzła powinno umożliwiać odzyskanie połączeń.
- Nowy uczestnik musi posiadać przynajmniej jedną metodę odnalezienia dostępnego węzła początkowego.

**Uzasadnienie:**

Dynamiczna zmiana uczestników jest podstawowym wymaganiem projektowanego systemu rozproszonego.

DHT zapewni mechanizm odnajdywania uczestników i dostawców danych bez utrzymywania centralnego katalogu wszystkich dokumentów.

Nie zakłada się jednak, że węzeł może dołączyć do całkowicie nieznanej sieci bez żadnej informacji początkowej.

### AD-06. Kademlia DHT jako mechanizm wyszukiwania

- DHT będzie obowiązkową częścią implementacji.
- Węzły będą ogłaszać dostępność wybranych bloków.
- Pozostali uczestnicy będą wyszukiwać dostawców na podstawie CID.
- Rozwiązanie ma umożliwiać dynamiczne odkrywanie dostawców treści.

**Uzasadnienie:**

Wykorzystanie wyłącznie statycznych adresów IP byłoby niewystarczające do demonstracji mechanizmów odkrywania zawartości w IPFS.

DHT oddziela identyfikację dokumentu od fizycznej lokalizacji węzła, który go przechowuje.

### AD-07. Własna implementacja CIDv1

- Zaimplementujemy tworzenie i odczytywanie CIDv1.
- Początkowo wykorzystamy kodek `raw` i SHA2-256.
- Zaimplementujemy odpowiednie kodowanie multihash, multicodec, varint i multibase.
- Poprawność będzie sprawdzana względem Kubo.

**Uzasadnienie:**

CID stanowi podstawę adresowania treścią w IPFS.

Samodzielna implementacja pozwoli zrozumieć mechanizm identyfikacji danych i przeprowadzić jednoznaczne testy zgodności.

SHA-256 będzie wykorzystywany do identyfikacji i weryfikacji integralności danych, a nie do ich szyfrowania.

### AD-08. Własny lokalny Blockstore

- Każdy węzeł będzie posiadał własny magazyn bloków.
- Bloki będą identyfikowane i wyszukiwane za pomocą CID.
- Zaimplementowane zostaną podstawowe operacje zapisu, odczytu, sprawdzania istnienia i usuwania lokalnego bloku.

**Uzasadnienie:**

Lokalny magazyn stanowi podstawową warstwę przechowywania danych.

Pozwala oddzielić zarządzanie danymi od komunikacji sieciowej i testować każdy mechanizm niezależnie.

### AD-09. Minimalna implementacja Bitswap

- Zaimplementowana zostanie wybrana funkcjonalność Bitswap zgodna ze specyfikacją.
- Węzeł będzie mógł żądać bloków i odpowiadać na żądania innych uczestników.
- Początkowy zakres obejmie podstawowe komunikaty i transfer pojedynczych bloków.
- Obsługa będzie rozwijana na podstawie testów z Kubo.

**Uzasadnienie:**

DHT służy do odnajdywania dostawców, natomiast Bitswap umożliwia wymianę bloków danych.

Zastosowanie zgodnego protokołu wymiany bloków jest istotne dla interoperacyjności ze standardowymi węzłami IPFS.

Nie zakładamy implementacji wszystkich optymalizacji Bitswap.

### AD-10. Podstawowy UnixFS

- Początkowo obsługiwane będą pojedyncze bloki `raw`.
- Następnie zostanie dodany odczyt podstawowych struktur UnixFS.
- Kolejnym krokiem będzie tworzenie plików wieloblokowych.
- Zostanie wybrany jeden deterministyczny sposób dzielenia plików na bloki i budowania DAG.

**Uzasadnienie:**

Dokumentacja medyczna może obejmować pliki większe niż pojedynczy blok.

UnixFS umożliwia reprezentowanie takich dokumentów jako struktur powiązanych bloków identyfikowanych przez CID.

Ograniczenie implementacji do podstawowych struktur pozwoli zachować rozsądny zakres pracy.

### AD-11. Pacjent zarządza własnym rejestrem dokumentacji

- Każdy pacjent posiada lokalny rejestr dokumentów i odpowiadających im CID.
- Placówka może tworzyć dokumenty przypisane do pacjenta.
- Pacjent może pobierać dokumenty i przechowywać własne kopie.
- Możliwe będzie przekazywanie manifestów dokumentów pomiędzy uczestnikami.

**Uzasadnienie:**

Koncepcja systemu zakłada model zorientowany na pacjenta, który zarządza informacjami o swojej dokumentacji.

CID jest identyfikatorem treści, a nie dowodem własności. Dlatego przypisanie dokumentów do pacjentów będzie realizowane na poziomie logiki EHR.

### AD-12. Replikacja danych

- Dokument będzie mógł być przechowywany na więcej niż jednym urządzeniu.
- Replikacja będzie realizowana jawnie, przez pobranie i zachowanie potrzebnych bloków.
- Będzie można sprawdzić dostępność dokumentu po odłączeniu jego pierwotnego dostawcy.

**Uzasadnienie:**

IPFS nie gwarantuje automatycznej, trwałej replikacji wszystkich dokumentów.

W systemie rozproszonym dostępność danych zależy od tego, czy aktywni uczestnicy posiadają odpowiednie bloki.

### AD-13. Brak IPNS i blockchaina

- Nie implementujemy IPNS.
- Nie implementujemy blockchaina.
- Nie stosujemy inteligentnych kontraktów.
- Zmiana dokumentu będzie oznaczać utworzenie nowego CID.
- Historia dokumentacji będzie przechowywana w rejestrze aplikacji EHR.

**Uzasadnienie:**

Podstawowym celem jest demonstracja mechanizmów IPFS oraz rozproszonego przechowywania i pobierania dokumentacji.

IPNS i blockchain nie są niezbędne do spełnienia przyjętych wymagań.

Rezygnacja z nich ogranicza złożoność systemu.

### AD-14. Brak rozbudowanego modułu cyberbezpieczeństwa

- Praca nie będzie koncentrować się na projektowaniu zabezpieczeń kryptograficznych.
- Testy będą wykonywane na dokumentacji testowej z zaakceptowanego przez promotora źródła.
- Integralność bloków będzie sprawdzana przez CID.
- Nie zakłada się tworzenia własnych algorytmów szyfrowania.

**Uzasadnienie:**

Praca dotyczy przede wszystkim architektury i funkcjonowania systemu rozproszonego.

Nie oznacza to, że IPFS zapewnia poufność danych. Hashowanie i CID nie stanowią szyfrowania.

Demonstrator nie jest przeznaczony do przechowywania rzeczywistych danych pacjentów ani do wdrożenia produkcyjnego.

### AD-15. Aplikacja konsolowa jako podstawowy interfejs

- Pierwsza wersja będzie programem CLI.
- Wszystkie operacje będzie można wykonywać z terminala.
- Interfejs webowy pozostanie opcjonalnym rozszerzeniem.
- Publiczny hosting nie będzie wymagany do działania systemu.

**Uzasadnienie:**

CLI umożliwia dokładne obserwowanie operacji i logów oraz ogranicza liczbę komponentów aplikacji.

Najważniejsza jest poprawność działania prototypu IPFS i EHR, a nie wygląd interfejsu.

### AD-16. Aplikacja mobilna jako kierunek rozwoju

- Architektura uwzględni możliwość wykorzystania urządzeń mobilnych w przyszłości.
- Początkowa demonstracja będzie wykonywana na komputerach.
- Nie zakłada się implementacji mobilnego klienta na pierwszym etapie.

**Uzasadnienie:**

Urządzenie mobilne odpowiada koncepcji pacjenta zarządzającego swoją dokumentacją.

Jednak pełna implementacja węzła mobilnego zwiększyłaby zakres pracy, szczególnie ze względu na ograniczenia połączeń sieciowych i pracy w tle.

---

## 3. Oczekiwana kompatybilność z Kubo

Jednym z kluczowych wymagań technicznych będzie interoperacyjność naszej implementacji z istniejącymi węzłami IPFS.

**Nie zakładamy pełnej zgodności funkcjonalnej z Kubo.**

Chcemy zapewnić zgodność w zakresie mechanizmów wykorzystywanych przez nasz system.

Oczekiwana interoperacyjność obejmuje:

- **CID:** Rust i Kubo poprawnie interpretują te same identyfikatory CIDv1.
- **Blockstore:** dane są zapisywane i odczytywane na podstawie zgodnego CID.
- **libp2p:** Rust i Kubo potrafią nawiązać połączenie P2P.
- **Kademlia DHT:** zgodnie skonfigurowane węzły mogą ogłaszać i odnajdywać dostawców.
- **Bitswap:** Kubo może pobrać obsługiwany blok z Rust, a Rust może pobrać blok z Kubo.
- **UnixFS:** Rust i Kubo mogą odczytać obsługiwane pliki utworzone przez drugą implementację.

Docelowo powinno być możliwe zastąpienie jednego z naszych węzłów węzłem Kubo bez zmiany pozostałych węzłów i bez zmiany formatu przechowywanych plików.

Zgodność ta dotyczy warstwy IPFS. Kubo nie będzie automatycznie obsługiwać naszego modelu pacjentów, historii medycznej ani własnych komend EHR.

### Zasada testowania interoperacyjności

Każdy komponent będzie testowany w trzech etapach:

1. Test jednostkowy własnej implementacji.
2. Test integracyjny pomiędzy dwoma procesami Rust.
3. Test interoperacyjności pomiędzy Rust i Kubo.

**Uwaga:** pozytywny wynik pojedynczego testu z Kubo potwierdza działanie określonego scenariusza, ale nie jest dowodem pełnej zgodności ze wszystkimi funkcjami specyfikacji IPFS.

---

## 4. Plan implementacji krok po kroku

### M0 — Przygotowanie środowiska

**Implementacja:**

- Utworzenie repozytorium GitHub.
- Konfiguracja Rust i Cargo.
- Utworzenie struktury projektu.
- Instalacja Kubo.
- Przygotowanie plików testowych.
- Przygotowanie środowiska testów jednostkowych i integracyjnych.

**Testy:**

- `cargo build`
- `cargo test`
- `ipfs version`
- `ipfs id`

**Kryterium ukończenia:** środowisko Rust i Kubo działa poprawnie.

### M1 — CIDv1

**Implementacja:**

- Obliczanie SHA-256.
- Kodowanie varint.
- Budowanie multihash.
- Budowanie CIDv1.
- Kodowanie multibase Base32.
- Odczyt CID.
- Weryfikacja zgodności danych z CID.

**Testy:**

- Porównanie CID wygenerowanego przez Rust z CID Kubo dla surowych bloków.
- Test pliku pustego.
- Test danych binarnych.
- Test zmienionej zawartości.
- Test nieprawidłowych identyfikatorów.

**Kryterium ukończenia:** Rust generuje identyczne CID jak Kubo dla tych samych bloków i ustawień.

### M2 — Blockstore

**Implementacja:**

- `put_block`
- `get_block`
- `has_block`
- `delete_block`
- `list_blocks`
- Trwałe przechowywanie danych na dysku.

**Testy:**

- Zapis i odczyt bloku.
- Odczyt po restarcie aplikacji.
- Próba pobrania nieistniejącego bloku.
- Weryfikacja integralności.
- Import bloku wyeksportowanego przez Kubo.

**Kryterium ukończenia:** lokalny magazyn poprawnie przechowuje i identyfikuje bloki.

### M3 — libp2p

**Implementacja:**

- Trwała tożsamość Peer ID.
- TCP.
- Noise.
- Yamux.
- Ping.
- Identify.
- Obsługa zdarzeń Swarm.

**Testy:**

- Połączenie Rust ↔ Rust.
- Połączenie Rust ↔ Kubo.
- Ping.
- Identify.
- Rozłączenie uczestnika.

**Kryterium ukończenia:** Rust może nawiązać standardowe połączenie libp2p z Kubo.

### M4 — Bootstrap i zarządzanie połączeniami

**Implementacja:**

- Lista bootstrap peerów.
- Zapamiętywanie znanych adresów.
- Automatyczne połączenia.
- Obsługa rozłączeń.
- Ponawianie połączeń.

**Testy:**

- Nowy węzeł łączy się z istniejącą siecią.
- Restart węzła bootstrap.
- Ponowne nawiązanie połączenia.
- Przywrócenie Peer ID po restarcie.

**Kryterium ukończenia:** węzły mogą dynamicznie dołączać i ponownie łączyć się z siecią.

### M5 — Kademlia DHT

**Implementacja:**

- Konfiguracja Kademlia.
- Tablice routingu.
- Bootstrap DHT.
- Ogłaszanie dostawcy CID.
- Wyszukiwanie dostawców CID.
- Obsługa zmian w tablicach routingu.

**Testy:**

- Rust ogłasza dostawcę.
- Inny Rust odnajduje dostawcę.
- Kubo odnajduje dostawcę ogłoszonego przez Rust.
- Rust odnajduje dostawcę ogłoszonego przez Kubo.
- Sprawdzanie działania przy zmianie liczby peerów.

**Kryterium ukończenia:** wyszukiwanie dostawców działa w zgodnie skonfigurowanej sieci IPFS.

### M6 — Bitswap Server

**Implementacja:**

- Obsługa protokołu `/ipfs/bitswap/1.2.0`.
- Kodowanie i odczyt wiadomości Protobuf.
- `Want-Have`.
- `Want-Block`.
- `Have`.
- `DontHave`.
- Wysyłanie zawartości bloków.

**Testy:**

- Rust odpowiada na zapytania o dostępność.
- Kubo pobiera blok z Rust.
- Weryfikacja zawartości pobranego bloku.
- Test nieistniejącego bloku.

**Kryterium ukończenia:** Kubo potrafi pobrać blok przechowywany wyłącznie przez Rust.

### M7 — Bitswap Client

**Implementacja:**

- Wysyłanie żądań bloków.
- Wykorzystanie wyników DHT.
- Odbieranie bloków.
- Weryfikacja CID.
- Zapis do Blockstore.
- Obsługa błędów, timeoutów i ponawiania żądań.

**Testy:**

- Rust pobiera blok z Kubo.
- Rust pobiera blok z innego węzła Rust.
- Test odłączonego dostawcy.
- Test uszkodzonego bloku.

**Kryterium ukończenia:** obsługiwane bloki mogą być pobierane dwukierunkowo między Rust i Kubo.

### M8 — UnixFS

**Implementacja:**

- Obsługa pojedynczych plików `raw`.
- Odczyt podstawowych struktur `dag-pb`.
- Odczyt metadanych UnixFS.
- Odtwarzanie plików wieloblokowych.
- Podział plików na bloki.
- Tworzenie zgodnej struktury DAG.

**Testy:**

- Rust pobiera PDF dodany przez Kubo.
- Kubo pobiera PDF dodany przez Rust.
- Weryfikacja bajtowej zgodności pliku.
- Testy plików o różnych rozmiarach.

**Kryterium ukończenia:** obie implementacje odczytują poprawnie pliki w obsługiwanym formacie UnixFS.

### M9 — Logika EHR

**Implementacja:**

- Model pacjenta.
- Model dokumentu.
- Powiązanie dokumentu z pacjentem.
- Przechowywanie rejestru CID.
- Tworzenie manifestów dokumentów.
- Obsługa kolejnych wersji dokumentu.
- Podstawowe operacje CLI.

**Testy:**

- Placówka dodaje dokument.
- Pacjent otrzymuje manifest.
- Pacjent pobiera dokument.
- Dokument jest prawidłowo przypisany do pacjenta.
- Kubo pobiera dokument na podstawie CID.

**Kryterium ukończenia:** aplikacja umożliwia zarządzanie przykładową dokumentacją medyczną.

### M10 — Dynamiczna sieć i replikacja

**Implementacja:**

- Replikacja wskazanych dokumentów.
- Zapisywanie lokalnych kopii.
- Ponowne ogłaszanie dostawców.
- Obsługa niedostępnych węzłów.
- Dołączanie nowych uczestników.
- Odłączanie i ponowne łączenie.

**Testy:**

- Nowy pacjent dołącza do sieci.
- Pacjent opuszcza sieć.
- Placówka zostaje odłączona.
- Inny węzeł udostępnia replikę.
- Pierwotna placówka ponownie dołącza.

**Kryterium ukończenia:** sieć zachowuje działanie przy zmianach uczestników, a dostępne repliki umożliwiają pobieranie dokumentów.

### M11 — Demonstracja końcowa

**Środowisko:**

- Komputer A — placówka medyczna.
- Komputer B — pacjent 1.
- Komputer C — pacjent 2.

**Scenariusz:**

1. Uruchomienie placówki A.
2. Dołączenie pacjenta B.
3. Dołączenie pacjenta C.
4. Opublikowanie dokumentu przez placówkę.
5. Przekazanie CID pacjentowi.
6. Wyszukanie dostawcy przez DHT.
7. Pobranie dokumentu przez Bitswap.
8. Odtworzenie pliku przez UnixFS.
9. Zapisanie repliki dokumentu.
10. Odłączenie placówki.
11. Pobranie dokumentu z alternatywnego węzła, bez wykorzystywania wcześniej zapisanej lokalnej kopii odbiorcy.
12. Ponowne dołączenie placówki.
13. Test zastąpienia wybranego węzła standardowym Kubo.

**Kryterium ukończenia:** wszystkie obowiązkowe scenariusze demonstracyjne działają i są udokumentowane.

---

## 5. Architektura modułowa repozytorium

Proponowana struktura repozytorium:

```text
ehr-ipfs/
├── Cargo.toml
├── Cargo.lock
├── README.md
│
├── crates/
│   ├── ipfs-cid/
│   │   └── src/
│   ├── ipfs-store/
│   │   └── src/
│   ├── ipfs-network/
│   │   └── src/
│   ├── ipfs-bitswap/
│   │   └── src/
│   ├── ipfs-unixfs/
│   │   └── src/
│   └── ehr-core/
│       └── src/
│
├── apps/
│   └── ehr-cli/
│       └── src/
│
├── tests/
│   ├── fixtures/
│   └── interoperability/
│
├── scripts/
│
└── docs/
    ├── IMPLEMENTATION_PLAN.md
    ├── architecture/
    ├── implementation/
    └── testing/
```

Każdy komponent będzie posiadał własny zakres odpowiedzialności, testy oraz dokumentację.

---

## 6. Dokumentowanie kolejnych etapów

Każdy ukończony etap implementacji powinien zostać opisany w osobnym pliku w katalogu `docs/implementation/`.

Przykładowo:

```text
docs/
├── phase-2-cid.md
├── phase-3-blockstore.md
...
└── phase-11-demonstration.md
```

Każdy dokument powinien zawierać:

- Cel implementowanego modułu.
- Wybrane specyfikacje i biblioteki.
- Opis przyjętej architektury.
- Zaimplementowane funkcje i struktury danych.
- Instrukcję uruchomienia.
- Testy jednostkowe.
- Testy integracyjne.
- Testy kompatybilności z Kubo.
- Wyniki i napotkane problemy.
- Ograniczenia implementacji.
- Wnioski.

## 7. Biblioteki i narzędzia

Wstępnie wybrane biblioteki:

| Biblioteka | Zastosowanie |
|---|---|
| `tokio` | Asynchroniczne operacje sieciowe |
| `libp2p` | Komunikacja P2P i Kademlia DHT |
| `sha2` | SHA-256 |
| `data-encoding` | Kodowanie Base32 |
| `prost` | Obsługa Protocol Buffers |
| `serde` | Serializacja danych |
| `serde_json` | Manifesty i konfiguracja |
| `clap` | Interfejs CLI |
| `thiserror` | Obsługa błędów |
| `tracing` | Logowanie |
| `tempfile` | Testy plikowe |

Wersje bibliotek zostaną ustalone podczas implementacji i zapisane w `Cargo.lock`.

---

## 8. Materiały referencyjne

Dokumentacja i specyfikacje:

- [CID Specification](https://specs.ipfs.tech/cid/)
- [Bitswap Protocol](https://specs.ipfs.tech/bitswap-protocol/)
- [UnixFS Specification](https://specs.ipfs.tech/unixfs/)
- [Kademlia DHT](https://specs.ipfs.tech/routing/kad-dht/)
- [rust-libp2p](https://docs.rs/libp2p/latest/libp2p/)
- [Kubo CLI Documentation](https://docs.ipfs.tech/reference/kubo/cli/)
- [IPFS Documentation](https://docs.ipfs.tech/)

Publikacje naukowe i dokumentacja EHR:

- Benet J., *IPFS — Content Addressed, Versioned, P2P File System*, 2014.
- Reen G. S., Mohandas M., Venkatesan S., *Decentralized Patient Centric e-Health Record Management System using Blockchain and IPFS*, 2020.
- [HL7 FHIR](https://hl7.org/fhir/).

Publikacja dotycząca blockchaina będzie wykorzystywana do analizy istniejących rozwiązań, nie jako wymaganie implementacyjne.

---

## 9. Kryteria ukończenia całego projektu

Projekt zostanie uznany za ukończony, jeśli:

- [ ] Działa własna implementacja CIDv1.
- [ ] Działa lokalny Blockstore.
- [ ] Węzły Rust komunikują się przez libp2p.
- [ ] Rust potrafi nawiązać połączenie z Kubo.
- [ ] DHT umożliwia wyszukiwanie dostawców.
- [ ] Bitswap umożliwia wymianę bloków.
- [ ] Rust i Kubo potrafią wymieniać obsługiwane bloki.
- [ ] UnixFS pozwala odtwarzać dokumenty wieloblokowe.
- [ ] Powstał działający program CLI.
- [ ] System obsługuje pacjentów i ich dokumentację.
- [ ] Dokumenty można udostępniać pomiędzy węzłami.
- [ ] Można dołączyć nowy węzeł.
- [ ] Można odłączyć istniejący węzeł.
- [ ] Replikacja umożliwia pobranie dokumentu od alternatywnego dostawcy.
- [ ] Demonstracja na trzech komputerach zakończyła się powodzeniem.
- [ ] Przeprowadzono i udokumentowano testy interoperacyjności z Kubo.

---

**Kolejność realizacji:**

`CIDv1 → Blockstore → libp2p → Bootstrap → Kademlia DHT → Bitswap Server → Bitswap Client → UnixFS → EHR → Dynamiczna sieć i replikacja → Demonstracja`


---

**Status dokumentu:** Wstępny plan implementacji. Decyzje techniczne mogą zostać doprecyzowane na podstawie wyników testów oraz ograniczeń wykrytych podczas implementacji.
