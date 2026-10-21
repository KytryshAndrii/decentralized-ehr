# Zdecentralizowany system EHR oparty na protokole IPFS

Eksperymentalna, minimalna implementacja wybranych mechanizmów protokołu IPFS (InterPlanetary File System) w języku Rust, zaprojektowana na potrzeby demonstracji zdecentralizowanego systemu elektronicznej dokumentacji medycznej (EHR).

Projekt ma charakter analityczno-badawczy (praca inżynierska) i opiera się na architekturze modułowej (Cargo Workspace). Implementacja weryfikowana jest pod kątem interoperacyjności z oficjalnym klientem IPFS — Kubo.

## Struktura projektu

*   `crates/` — Wewnętrzne moduły systemu (np. `ipfs-cid`, docelowo: blockstore, network, bitswap).
*   `apps/` — Aplikacje końcowe (np. `ehr-cli`).
*   `tests/test-runner/` — Zcentralizowany pakiet do uruchamiania testów czarnoskrzynkowych i testów interoperacyjności z zewnętrznymi procesami IPFS.
*   `tests/mocks/` — Współdzielone atrapy plików medycznych (JSON, XML, TTL) i graficznych (BMP, SVG, TIFF) dla testów.

## Wymagania

*   **Rust** (wersja stabilna 2024+)
*   **Kubo** (IPFS v0.43.1+) zainstalowane lokalnie i dostępne w zmiennej systemowej `PATH`.
*   Zainicjalizowane lokalne repozytorium IPFS (do testów interoperacyjności): `ipfs init`

## Uruchamianie testów

Projekt posiada rozbudowaną suitę testową obejmującą testy jednostkowe, integracyjne (lifecycle, data corruption) oraz testy zgodności z demonem Kubo.

### 1. Szybkie uruchomienie całości (Skrypt)
Najwygodniejsza metoda uruchamiająca wszystkie testy w odpowiedniej kolejności i zabezpieczająca przed konfliktami wielowątkowości (blokada `repo.lock` w Kubo):
```bash
./scripts/testing/run-rust-tests.sh
```

### 2. Manualne polecenia narzędzia Cargo

*   **Uruchomienie wszystkich testów w całym projekcie (Workspace):**
    ```bash
    cargo test
    ```
*   **Testy jednostkowe wybranego modułu (np. CID):**
    ```bash
    cargo test -p ipfs-cid
    ```
*   **Pełny cykl testów czarnoskrzynkowych (Integracyjne + Interoperacyjności):**
    ```bash
    cargo test -p test-runner
    ```
*   **Tylko testy integracyjne (sprawdzające wewnętrzne mechanizmy):**
    ```bash
    cargo test -p test-runner --test cid-integration
    ```
*   **Tylko testy interoperacyjności (wymaga Kubo; wymuszenie trybu jednowątkowego, aby uniknąć błędu `someone else has the lock`):**
    ```bash
    cargo test -p test-runner --test cid-interop -- --test-threads=1
    ```
