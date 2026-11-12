# 06 — Warstwa bezpieczeństwa: szyfrowanie (GPG/OpenPGP) + autoryzacja (UCAN)

Pełna analiza: `docs/phase-1.1-planning-role-models.md`. Ten plik zawiera skrót roboczy i aktualną decyzję.

## 1. Decyzja (D-15, 2026-11-12)

Ostatni etap implementacji, **wymagany w DoD**: minimalne szyfrowanie + autoryzacja **UCAN + GPG** jako **dodatkowa warstwa na gotowym węźle IPFS**. Warstwa IPFS (CID, Blockstore, DHT, Bitswap, UnixFS) **nie zmienia się**: szyfrowanie odbywa się **przed** `put` i UnixFS, a autoryzacja jest sprawdzana w warstwie EHR/serwisu.

```text
               ehr-cli (rola: pacjent | placówka)
                         │
     ┌───────────────────┼────────────────────┐
     │ UCAN: CZY WOLNO?  │ GPG: CZY MOGĘ       │
     │ (delegacja, akcja,│ ODCZYTAĆ? (szyfr    │
     │ zasób, ważność)   │ hybrydowy, podpis)  │
     └───────────────────┼────────────────────┘
                         │ szyfrogram + podpis
                  ehr-core (manifest, rejestr)
                         │
          ipfs-unixfs → ipfs-store → ipfs-bitswap/dht   (bez zmian)
```

## 2. Co rozwiązuje który mechanizm

| | UCAN | GPG / OpenPGP (RFC 9580) |
|---|---|---|
| Pytanie | „Czy podmiot ma prawo wykonać operację X na zasobie Y?” | „Czy podmiot ma klucz, by odczytać treść?” + „Kto podpisał?” |
| Szyfruje treść | **nie** | **tak** (klucz symetryczny + opakowanie dla każdego odbiorcy) |
| Delegacja, czas ważności | tak | nie |
| Podpis dokumentu | podpis tokenu | podpis dokumentu (detached/inline) |

- **Sam UCAN nie chroni poufności**: znając CID, można pobrać jawny plik z IPFS z pominięciem aplikacji.
- **Sam GPG nie definiuje ról**: poprawny podpis nie znaczy, że podpisujący jest lekarzem. Potrzebny jest rejestr zaufanych kluczy placówek.
- Hash i CID to **nie szyfrowanie**. Integralność daje CID, autentyczność podpis, poufność szyfrowanie.

## 3. Role (ten sam program, inne uprawnienia)

| Operacja | Pacjent | Placówka / lekarz |
|---|---|---|
| Odczyt własnej dokumentacji | tak | wg uprawnień (UCAN od pacjenta) |
| Odczyt cudzej | nie | tylko z dostępem (klucz + UCAN) |
| Dodanie własnego pliku | tak | tak |
| Wystawienie oficjalnego dokumentu | nie | tak (podpis + klucz w rejestrze zaufanych) |
| Uznanie podpisu za podpis lekarza | nie automatycznie | po weryfikacji roli |
| Udostępnienie dokumentu innym | tak (delegacja UCAN + ponowne opakowanie klucza) | wg zasad |

## 4. Scenariusze testowe (T01–T10 z phase-1.1)

T01 placówka szyfruje dla pacjenta A · T02 szyfrogram → CID · T03 A pobiera · T04 A odszyfrowuje · T05 B pobiera szyfrogram A (może się udać) · **T06 B nie odszyfruje** · T07 podpis lekarza zweryfikowany · **T08 podpis pacjenta nie jest uznany za lekarski** · T09 modyfikacja wykryta (CID/podpis) · T10 pobranie z drugiego węzła.
Do tego dla UCAN: token wygasły, zły audience, akcja poza zakresem i łańcuch delegacji odrzucone.

## 5. Ograniczenia (do pracy)

Cofnięcie dostępu nie usuwa pobranych kopii (dla przyszłych wersji: nowy klucz i ponowne szyfrowanie); utrata klucza prywatnego = utrata dostępu (potrzebny backup); zarządzanie zaufaniem do kluczy (lokalna lista zaufanych); **prywatność metadanych**: nie publikować nazwisk, PESEL ani rozpoznań w nazwach plików i manifestach jawnych; rekord providera w DHT ujawnia, kto ma dany multihash.

## 6. Otwarte decyzje (do ustalenia z autorem przed implementacją)

- **D01 Klucze:** generowane przez aplikację czy importowane? Gdzie przechowywane (np. obok `./tmp/node-key.pb`)? Jak weryfikować publiczne? *Uwaga:* klucz libp2p (Ed25519, multicodec `libp2p-key` 0x72) można rozważyć jako tożsamość UCAN (did:key), co łączy PeerId z tożsamością autoryzacyjną (notatka autora: „libp2p-key może się przydać przy autentykacji”).
- **D02 Model uprawnień:** stały dostęp lekarza czy zgoda per dokument; placówka zachowuje kopię?; placówka vs pojedynczy lekarz.
- **D03 Przechowywanie:** kto pinuje, gdzie jest rejestr pacjent ↔ dokument ↔ CID.
- **D04 Podpis:** lekarz czy placówka; czy pacjent podpisuje własne pliki; jak odróżnić dokument oficjalny od prywatnego.
- **D05 Udostępnianie:** ponowne szyfrowanie czy opakowanie klucza dla nowego odbiorcy; limity czasowe (UCAN `exp`).
- **Biblioteki Rust:** do wyboru (np. `sequoia-openpgp` / `rpgp` (`pgp`) dla OpenPGP; `rs-ucan` / `ucan` crate dla UCAN). Wybór zweryfikować z aktualną dokumentacją i licencją; nie pisać własnej kryptografii.
