# 09 — Notatki autora, odpowiedzi na pytania, otwarte kwestie

## 1. Odpowiedzi na pytania autora (wcześniej zadane, zachowane do pracy)

- **Od jakiego rozmiaru `ipfs add` tworzy strukturę UnixFS z wieloma blokami?** Domyślny chunker Kubo to `size-262144` (**256 KiB**). Plik większy niż 1 chunk staje się DAG-iem `dag-pb` (UnixFS File) z liśćmi. Plik ≤ 256 KiB to jeden blok, ale jego **CID zależy od opcji**: przy domyślnym CIDv0 jest opakowany w `dag-pb` (`Qm...`, inny niż raw); przy `--cid-version=1` (raw-leaves włączone domyślnie) mały plik dostaje CID `raw` (`bafkrei...`), identyczny z `ipfs block put`. Dlatego porównania z Kubo robimy na surowych blokach albo z jawnymi opcjami (`--cid-version=1 --raw-leaves --chunker=size-262144`). *Zweryfikować w docs Kubo przy M8.*
- **Jak projektować modularnie bez OOP?** Traits + generyki/`dyn Trait` + kompozycja + moduły z widocznością; zob. `02-...` §4.
- **Czy kierunek połączenia ogranicza wysyłanie plików?** Nie. Po połączeniu oba końce mogą otwierać strumienie (Yamux).
- **Czy XOR znajduje geograficznie najbliższy węzeł?** Nie (logiczna odległość); zob. `05-...` §7.
- **LAN przez VPN?** Tak, VPN L3 z routowanymi adresami; zob. `05-...` §6.
- **Czy M6/M7 Bitswap server/client to DHT server/client?** Nie; zob. D-24.

## 2. Notatki autora (do wykorzystania w pracy)

- Implementacja minimalnego Bitswap to nie tylko `send()`/`receive()`: negocjacja protokołu (multistream-select), ramkowanie (varint length prefix), protobuf, żądania i odpowiedzi, **weryfikacja bloków**.
- **Każdy komponent sprawdzamy względem istniejącej implementacji IPFS od razu, a nie na końcu.** Tabela interop:

| Mechanizm | Test interoperacyjności |
|---|---|
| CID | Rust i Kubo wyliczają ten sam CID dla identycznych bloków |
| libp2p | węzeł Rust zestawia połączenie z Kubo |
| DHT | węzeł odnajduje dostawców przez kompatybilną DHT |
| Bitswap | Rust pobiera blok od Kubo i odwrotnie |
| UnixFS | plik zapisany przez Rust da się odczytać przez Kubo |
| Dostępność | dokument da się pobrać po wyłączeniu pierwotnego dostawcy, jeśli istnieje replika |

- Cytat do pracy: *„IPFS is used as a storage layer for some decentralized applications and Web3 projects, including NFT platforms and blockchain-based services, because content addressing and distributed storage can improve data persistence and reduce reliance on single centralized servers.”* (źródło do ustalenia przy cytowaniu).
- Definicja protokołu komunikacyjnego (Wikipedia „Communication protocol”): reguły, składnia, semantyka, synchronizacja i odzyskiwanie po błędach.
- Rust: brak dziedziczenia implementacji, więc kompozycja + traits (jak interfejsy w Javie lub mixiny).
- Multicodec: `raw 0x55` (surowe bajty), `dag-pb 0x70` (UnixFS, protobuf, Links + Data), `dag-cbor 0x71` (binarny JSON z linkami CID), `dag-json 0x0129` (`{"/": "bafy..."}`), **`libp2p-key 0x72`** (klucz publiczny węzła/IPNS; *może się przydać przy autentykacji*). Formaty plików (XML, TTL, BMP, ...) nie dostają własnych multicodec, tylko `raw` lub `dag-pb`.
- Multibase: ten sam binarny CID w różnych kodowaniach (base32 `b`, base58btc `z`, ...). RFC 4648: Base16, Base32 (A–Z, 2–7), Base64 (+ base64url). Varint: LEB128 (unsigned).
- Konwersja CIDv1→v0 jest możliwa tylko dla `dag-pb`.
- **Wymagania wobec implementacji IPFS (specs.ipfs.tech/architecture/principles):** MUST adresowanie CID; MUST operacje (pobieranie, udostępnianie, indeksowanie) po CID; **MUST weryfikować, że rozwiązany CID pasuje do bajtów**; SHOULD nazywać ważne zasoby CID-ami; SHOULD wystawiać logiczne jednostki (plik, katalog) jako CID; SHOULD inkrementalna weryfikowalność; MAY dowolny transport (transport nie dyktuje mapowania CID→treść). *Nasz Blockstore spełnia MUST weryfikacji przy put/get; Bitswap client musi weryfikować każdy odebrany blok.*
- PIK HL7 CDA: polski standard dokumentów medycznych (do części teoretycznej o formatach EHR; mocki projektu to HL7 FHIR).

## 3. Otwarte kwestie (zapytaj autora, nie zgaduj)

1. **D-22 LAN-only vs WAN default + `--dev-lan-dht`.** Kod i najnowszy dokument mówią LAN-only. Potwierdzić, że wersja z rozmowy ChatGPT jest porzucona.
2. Pytania z planowania pracy: (a) poziom decentralizacji: tylko pliki czy też metadane i rejestr? (b) zakres bezpieczeństwa: rozstrzygnięty, GPG + UCAN obowiązkowe; (c) zakres aplikacji: CLI z fikcyjnymi pacjentami (obecny kierunek) czy tylko narzędzie plikowe?
3. Implementacja Bitswap na libp2p 0.57: `libp2p-stream` vs własny `ConnectionHandler`.
4. Biblioteki OpenPGP i UCAN oraz decyzje D01–D05 (`06-...` §6).
5. Zakres cyklu życia danych (pinning/GC/delete) w M10 czy tylko w części teoretycznej.
