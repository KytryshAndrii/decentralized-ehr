# 01 — Zakres pracy dyplomowej i Definition of Done

## 1. Karta tematu (USOS/APD, 1:1)

**Tytuł:** Implementacja zdecentralizowanego systemu EHR opartego o protokół IPFS

**CEL PRACY:** Analiza możliwości zastosowania protokołu IPFS w systemach EHR oraz przygotowanie demonstracyjnej implementacji rozwiązania wykorzystującego wybrane założenia IPFS.

**ASPEKT INŻYNIERSKI:** Zaprojektowanie uproszczonej architektury rozwiązania EHR wykorzystującego wybrane założenia IPFS oraz przygotowanie demonstracyjnej implementacji umożliwiającej zapis, identyfikację, pobieranie i udostępnianie przykładowych danych lub plików medycznych w sposób zdecentralizowany.

**Współpraca z przemysłem:** Nie. **Zgoda komisji bioetycznej:** Nie.

**ZADANIA DO WYKONANIA:**
1. Analiza literatury dotyczącej systemów EHR oraz zdecentralizowanego przechowywania danych.
2. Omówienie podstawowych założeń działania protokołu IPFS.
3. Przegląd wybranych rozwiązań wykorzystujących IPFS w kontekście danych medycznych.
4. Identyfikacja zalet i ograniczeń zastosowania IPFS w systemach EHR.
5. Opracowanie uproszczonej architektury rozwiązania EHR wykorzystującego wybrane założenia IPFS.
6. Wykonanie demonstracyjnej implementacji wybranych mechanizmów IPFS.
7. Demonstracja działania rozwiązania na przykładzie zapisu, identyfikacji, pobierania i udostępniania przykładowych danych lub plików.

**LITERATURA (z karty):**
1. Benet J., *IPFS - Content Addressed, Versioned, P2P File System*, arXiv:1407.3561, 2014.
2. Reen G. S., Mohandas M., Venkatesan S., *Decentralized Patient Centric e-Health Record Management System using Blockchain and IPFS*, arXiv:2009.14285, 2020.
3. Guasch Repullés M., *IPFS as a Foundation for Anonymous File Storage*, Universitat Oberta de Catalunya.
4. IPFS Docs, *InterPlanetary Name System — Mutability in IPFS*, https://docs.ipfs.tech/concepts/ipns/

**UWAGI:** Praca ma charakter analityczno-projektowy. Wykorzystuje istniejące koncepcje, bez tworzenia nowych algorytmów ani protokołów. Implementacja jest demonstracyjna i nie obejmuje pełnego systemu produkcyjnego.

## 2. Jak autor widzi implementację (interpretacja zatwierdzona w rozmowach)

- **Własna minimalna implementacja IPFS w Rust** (nie wrapper na API Kubo). Kubo służy tylko jako referencja do testów interop.
- Biblioteki standardowe są dozwolone (libp2p: TCP/Noise/Yamux/Identify/Ping/Kad; sha2, multibase, unsigned-varint, rusty-leveldb, tokio). Własna logika obejmuje CID, Blockstore, integrację sieci, Bitswap, UnixFS, EHR i warstwę bezpieczeństwa.
- Wszystkie węzły to ten sam program `ehr-cli` z inną konfiguracją.
- Model **zorientowany na pacjenta**: pacjent zarządza rejestrem swoich CID, a placówka wystawia dokumenty.
- CID to identyfikator treści, a nie dowód własności ani mechanizm dostępu. Przypisanie do pacjenta i uprawnienia realizuje warstwa EHR i bezpieczeństwa.
- Każda zmiana dokumentu daje nowy CID. Historię prowadzi rejestr EHR (bez IPNS i bez blockchaina).

## 3. Definition of Done CAŁEJ pracy (implementacja)

Projekt jest ukończony, gdy:

- [x] Własny CIDv1 (raw/sha2-256, multibase base32) daje CID identyczne z Kubo.
- [x] Trwały lokalny Blockstore (LevelDB) z weryfikacją integralności.
- [x] Węzły Rust komunikują się przez libp2p; Rust łączy się z Kubo.
- [x] Kademlia DHT: provide/find providers (LAN DHT), Client/Server; testy jednostkowe i interop z Kubo PASS.
- [ ] **Bitswap server** (M6): Kubo pobiera blok przechowywany tylko przez Rust.
- [ ] **Bitswap client** (M7): Rust pobiera blok od Kubo i od innego Rusta (DHT → Bitswap → weryfikacja CID → Blockstore).
- [ ] **UnixFS** (M8): pliki wieloblokowe (dag-pb + raw leaves) zgodne z Kubo w obie strony.
- [ ] **Logika EHR** (M9): pacjent, dokument, manifest, rejestr CID, wersje dokumentu, CLI.
- [ ] **Dynamiczna sieć i replikacja** (M10): dołączanie i odłączanie węzłów, pobranie z repliki po wyłączeniu placówki.
- [ ] **Warstwa bezpieczeństwa (warunek konieczny wg autora):** minimalne szyfrowanie dokumentów (GPG/OpenPGP, hybrydowe, wielu odbiorców) **+ autoryzacja UCAN** (delegacja uprawnień pacjent → lekarz/placówka) jako dodatkowa warstwa na gotowym węźle IPFS. Pacjent i placówka różnią się uprawnieniami.
- [ ] Demonstracja na 3 komputerach (placówka A, pacjent B, pacjent C) według scenariusza M11 + test podmiany węzła na Kubo.
- [ ] Każdy etap opisany w `docs/phase-N-*.md` (cel, specyfikacje, architektura, problemy i rozwiązania, testy, ograniczenia, wnioski).

## 4. Mapowanie zadań z karty na etapy

| Zadanie z karty | Gdzie realizowane |
|---|---|
| 1–4 (analiza, literatura, zalety/ograniczenia) | Część teoretyczna pracy; materiał w `docs/phase-1*.md`, `07-data-lifecycle.md`, `05-network-dht.md` (XOR, LAN/WAN, prywatność metadanych) |
| 5 (architektura) | `docs/phase-1-planning.md`, `docs/phase-1.1-planning-role-models.md`, `02-...` |
| 6 (implementacja mechanizmów IPFS) | M1–M8 |
| 7 (demonstracja: zapis, identyfikacja, pobieranie, udostępnianie) | Zapis = Blockstore/UnixFS, identyfikacja = CID, pobieranie = DHT + Bitswap, udostępnianie = manifest + GPG/UCAN; M9–M11 |
