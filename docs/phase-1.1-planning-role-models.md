# UCAN vs GPG – autoryzacja i szyfrowanie w zdecentralizowanym systemie EHR opartym na IPFS

## 1. Cel dokumentu

Celem dokumentu jest analiza i porównanie dwóch mechanizmów kryptograficznych:

- UCAN (User Controlled Authorization Network)
- GPG (GNU Privacy Guard)

Analiza dotyczy możliwości wykorzystania obu technologii w demonstracyjnym systemie elektronicznej dokumentacji medycznej (EHR), opartym na protokole IPFS (InterPlanetary File System).

Głównym problemem projektowym jest rozdzielenie uprawnień pomiędzy pacjentem a placówką medyczną lub lekarzem, bez konieczności stosowania centralnego serwera autoryzacji.

System powinien zapewniać:

1. Niezależne działanie węzłów IPFS pacjentów i placówek medycznych.
2. Możliwość zapisywania i pobierania dokumentów medycznych.
3. Poufność dokumentacji medycznej.
4. Uniemożliwienie pacjentowi odczytania dokumentów innego pacjenta.
5. Możliwość identyfikacji podmiotu wystawiającego dokument medyczny.
6. Rozdzielenie uprawnień do tworzenia i odczytywania dokumentacji.
7. Brak centralnego serwera odpowiadającego za przechowywanie dokumentów i autoryzację użytkowników.

---

## 2. Założenia architektury EHR

W projektowanym rozwiązaniu każdy uczestnik sieci posiada własny węzeł IPFS.

### 2.1. Pacjent

Pacjent posiada:

- własny węzeł IPFS,
- aplikację umożliwiającą zarządzanie dokumentami medycznymi,
- własną parę kluczy kryptograficznych,
- możliwość pobierania dokumentów przeznaczonych dla niego,
- możliwość udostępniania dokumentów innym uprawnionym podmiotom.

### 2.2. Placówka medyczna / lekarz

Placówka medyczna posiada:

- własny węzeł IPFS,
- aplikację umożliwiającą wystawianie dokumentów medycznych,
- własną parę kluczy kryptograficznych,
- możliwość podpisywania dokumentów,
- możliwość szyfrowania dokumentów dla wybranych pacjentów,
- możliwość pobierania dokumentów, do których posiada odpowiedni dostęp.

### 2.3. Komunikacja

Komunikacja między uczestnikami odbywa się przy wykorzystaniu IPFS.

Przykładowa topologia:

```text
              SIEĆ IPFS (P2P)
                     |
         +-----------+-----------+
         |           |           |
     Pacjent A   Pacjent B   Placówka L
         |           |           |
      IPFS A       IPFS B      IPFS L
         |           |           |
      Klucze A     Klucze B    Klucze L
```

Ważne: brak centralnego serwera nie oznacza braku zasad zaufania. System nadal musi określać, które klucze publiczne należą do pacjentów, lekarzy i placówek.

---

# 3. UCAN – User Controlled Authorization Network

## 3.1. Czym jest UCAN?

UCAN (User Controlled Authorization Network) jest mechanizmem zdecentralizowanej autoryzacji opartym na kryptograficznie podpisywanych tokenach.

Pozwala użytkownikom nadawać i delegować określone uprawnienia bez konieczności odpytywania centralnego serwera autoryzacji.

UCAN umożliwia określenie:

- kto nadaje uprawnienie,
- komu uprawnienie jest nadawane,
- jakiego zasobu dotyczy,
- jakie operacje są dozwolone,
- jak długo uprawnienie pozostaje ważne,
- czy i w jakim zakresie uprawnienie może być delegowane dalej.

Tokeny UCAN mogą być weryfikowane lokalnie przy użyciu kryptografii asymetrycznej.

UCAN jest mechanizmem autoryzacji, a nie mechanizmem szyfrowania dokumentów.

## 3.2. Przykład zastosowania UCAN w EHR

Pacjent A chce zezwolić lekarzowi L na odczyt wybranej dokumentacji medycznej.

Pacjent generuje delegację, która określa:

- wystawcę: pacjent A,
- odbiorcę: lekarz L,
- zasób: dokumentacja pacjenta A,
- operację: odczyt,
- termin ważności: określona data.

Uproszczona reprezentacja koncepcyjna:

```json
{
  "issuer": "pacjent_A",
  "audience": "lekarz_L",
  "resource": "ehr/pacjent_A",
  "action": "read",
  "expires": "2026-11-01"
}
```

Powyższy przykład przedstawia koncepcję delegacji, a nie rzeczywisty format tokenu UCAN.

Schemat działania:

```text
Pacjent A
    |
    | Podpisuje delegację UCAN
    v
Token UCAN
    |
    | Przekazanie lekarzowi
    v
Lekarz L
    |
    | Żądanie wykonania operacji
    v
Aplikacja EHR
    |
    | Weryfikacja tokenu
    | Weryfikacja podpisu
    | Weryfikacja uprawnienia
    | Weryfikacja odbiorcy
    | Weryfikacja ważności
    v
Zezwolenie / odmowa wykonania operacji
```

Weryfikacja nie musi wymagać komunikacji z centralnym serwerem.

## 3.3. Integracja UCAN z IPFS

UCAN może zostać wykorzystany w warstwie aplikacji EHR działającej nad IPFS.

Przykładowy proces:

1. Pacjent tworzy delegację UCAN.
2. Delegacja zostaje podpisana kluczem prywatnym pacjenta.
3. Lekarz otrzymuje token.
4. Lekarz wysyła żądanie pobrania dokumentu do aplikacji lub usługi EHR.
5. Aplikacja weryfikuje token.
6. Jeżeli uprawnienia są poprawne, aplikacja wykonuje operację z użyciem IPFS.

Istotne ograniczenie:

UCAN nie ogranicza bezpośrednio dostępu do plików w publicznej sieci IPFS.

Jeżeli dokument jest przechowywany w postaci jawnej, osoba posiadająca jego CID może próbować pobrać go bezpośrednio z IPFS, pomijając aplikację sprawdzającą token UCAN.

Z tego powodu UCAN nie zapewnia samodzielnie poufności dokumentacji medycznej.

## 3.4. Zalety UCAN

1. **Decentralizacja autoryzacji**

   Nie jest wymagany centralny serwer wydający decyzje autoryzacyjne dla każdej operacji.

2. **Delegowanie uprawnień**

   Pacjent może nadać lekarzowi prawo dostępu do określonych zasobów.

3. **Szczegółowe definiowanie uprawnień**

   Możliwe jest rozróżnienie operacji, takich jak:

   - odczyt,
   - tworzenie,
   - aktualizowanie,
   - delegowanie.

4. **Ograniczenia czasowe**

   Token może posiadać określony termin ważności.

5. **Kryptograficzna weryfikacja**

   Poprawność delegacji można sprawdzać na podstawie podpisów cyfrowych.

6. **Możliwość tworzenia łańcuchów delegacji**

   Uprawniony podmiot może przekazać ograniczoną część swoich uprawnień innemu podmiotowi, jeżeli pozwalają na to zasady delegacji.

## 3.5. Wady UCAN

1. Nie zapewnia szyfrowania dokumentów.
2. Nie chroni treści przechowywanych jawnie w IPFS.
3. Wymaga implementacji logiki autoryzacji w aplikacji.
4. Wymaga zarządzania tożsamościami kryptograficznymi.
5. Cofanie dostępu przed wygaśnięciem tokenu wymaga dodatkowego mechanizmu.
6. Weryfikacja łańcuchów delegacji zwiększa złożoność implementacji.
7. Nie gwarantuje, że niezależny uczestnik sieci będzie respektował reguły aplikacji.

## 3.6. Główne ograniczenie UCAN

UCAN odpowiada na pytanie:

> Czy użytkownik jest uprawniony do wykonania określonej operacji?

Nie odpowiada natomiast na pytanie:

> Czy użytkownik posiada możliwość kryptograficznego odczytania zawartości dokumentu?

Dlatego UCAN bez dodatkowego szyfrowania nie spełnia podstawowego wymagania poufności dokumentów EHR.

---

# 4. GPG – GNU Privacy Guard

## 4.1. Czym jest GPG?

GPG (GNU Privacy Guard) jest narzędziem implementującym standard OpenPGP.

Umożliwia:

- szyfrowanie plików,
- odszyfrowywanie plików,
- generowanie kluczy kryptograficznych,
- podpisywanie danych,
- weryfikowanie podpisów cyfrowych.

GPG wykorzystuje kryptografię asymetryczną oraz szyfrowanie hybrydowe.

Każdy uczestnik posiada parę kluczy:

- klucz publiczny,
- klucz prywatny.

Klucz publiczny może być udostępniany innym uczestnikom.

Klucz prywatny powinien pozostawać pod wyłączną kontrolą właściciela.

## 4.2. Mechanizm szyfrowania

GPG wykorzystuje szyfrowanie hybrydowe.

W uproszczeniu:

1. Generowany jest losowy klucz symetryczny.
2. Dokument jest szyfrowany kluczem symetrycznym.
3. Klucz symetryczny jest zabezpieczany dla wskazanych odbiorców z wykorzystaniem ich kluczy publicznych.
4. Powstaje zaszyfrowany plik.
5. Uprawniony odbiorca wykorzystuje swój klucz prywatny do odzyskania klucza symetrycznego.
6. Odbiorca odszyfrowuje dokument.

Dzięki temu jeden dokument może zostać zaszyfrowany dla wielu odbiorców.

## 4.3. Przykład zastosowania GPG w EHR

Placówka medyczna przygotowuje dokument pacjenta A.

Dokument powinien być czytelny dla:

- pacjenta A,
- uprawnionego lekarza L.

Dokument nie powinien być czytelny dla pacjenta B.

Przebieg:

```text
               Placówka medyczna
                       |
                       v
               Wynik_badania.pdf
                       |
                       v
                  GPG Encrypt
                       |
             +---------+---------+
             |                   |
       Klucz publiczny     Klucz publiczny
         pacjenta A          lekarza L
             |                   |
             +---------+---------+
                       |
                       v
              Wynik_badania.gpg
                       |
                       v
                   IPFS Add
                       |
                       v
                       CID
                       |
            +----------+----------+
            |          |          |
        Pacjent A   Pacjent B   Lekarz L
            |          |          |
        Pobiera      Pobiera     Pobiera
            |          |          |
        Odszyfruje    Odmowa    Odszyfruje
```

Pacjent B może pobrać zaszyfrowany plik, jeśli jest on dostępny w IPFS i zna odpowiedni CID.

Nie może jednak odszyfrować dokumentu bez klucza prywatnego jednego z uprawnionych odbiorców lub uzyskania dostępu do równoważnego materiału kryptograficznego.

## 4.4. Integracja GPG z IPFS

GPG działa niezależnie od protokołu IPFS.

Nie wymaga modyfikowania samego IPFS.

Szyfrowanie odbywa się przed dodaniem dokumentu do sieci.

### Krok 1. Szyfrowanie dokumentu

```bash
gpg --encrypt \
  --recipient pacjent_A \
  --recipient lekarz_L \
  --output wynik.pdf.gpg \
  wynik.pdf
```

Identyfikatory odbiorców muszą odpowiadać wcześniej zaimportowanym i poprawnie zweryfikowanym kluczom publicznym.

### Krok 2. Dodanie dokumentu do IPFS

```bash
ipfs add wynik.pdf.gpg
```

Przykładowy wynik:

```text
added bafy... wynik.pdf.gpg
```

Zwrócony CID identyfikuje zaszyfrowany plik.

### Krok 3. Pobranie dokumentu

```bash
ipfs get <CID> -o wynik.pdf.gpg
```

### Krok 4. Odszyfrowanie dokumentu

```bash
gpg --decrypt \
  --output wynik.pdf \
  wynik.pdf.gpg
```

Jeżeli użytkownik nie posiada odpowiedniego klucza prywatnego, odszyfrowanie zakończy się niepowodzeniem.

## 4.5. Podpisy cyfrowe GPG

GPG umożliwia również podpisywanie dokumentów.

Podpis cyfrowy pozwala wykryć zmianę podpisanych danych i zweryfikować, że podpis został utworzony kluczem odpowiadającym określonemu kluczowi publicznemu.

Przykład:

```bash
gpg --detach-sign \
  --output wynik.pdf.sig \
  wynik.pdf
```

Weryfikacja:

```bash
gpg --verify wynik.pdf.sig wynik.pdf
```

W systemie EHR podpisy mogą służyć do rozróżnienia:

- dokumentu wystawionego przez zaufanego lekarza,
- dokumentu dodanego przez pacjenta,
- dokumentu zmodyfikowanego po podpisaniu.

Ważne:

Poprawny podpis cyfrowy nie oznacza automatycznie, że podpisujący jest lekarzem.

System musi posiadać mechanizm wiarygodnego powiązania klucza publicznego z tożsamością i rolą osoby podpisującej.

Może to być lokalnie zarządzany rejestr zaufanych kluczy placówek i lekarzy lub inny mechanizm ustanawiania zaufania.

## 4.6. Zalety GPG

1. **Poufność dokumentów**

   Nieuprawniony uczestnik nie może odczytać poprawnie zaszyfrowanego dokumentu bez odpowiedniego klucza.

2. **Brak konieczności centralizacji**

   Szyfrowanie i odszyfrowywanie mogą odbywać się lokalnie.

3. **Prosta integracja z IPFS**

   Do IPFS trafia zaszyfrowany plik.

4. **Obsługa wielu odbiorców**

   Jeden dokument można zaszyfrować dla pacjenta i lekarza.

5. **Podpisy cyfrowe**

   Możliwa jest weryfikacja integralności i kryptograficznego pochodzenia dokumentów.

6. **Dojrzała technologia**

   GPG posiada rozbudowaną dokumentację i narzędzia.

7. **Możliwość wykorzystania w demonstracyjnym prototypie**

   Podstawową funkcjonalność można sprawdzić przy pomocy interfejsu CLI.

## 4.7. Wady GPG

1. Nie jest kompletnym systemem autoryzacji.
2. Nie definiuje bezpośrednio ról pacjenta i lekarza.
3. Wymaga zarządzania kluczami kryptograficznymi.
4. Utrata klucza prywatnego może uniemożliwić odzyskanie dokumentów.
5. Udostępnienie dokumentu nowemu odbiorcy wymaga operacji na kluczach lub ponownego przygotowania zaszyfrowanych danych.
6. Cofnięcie uprawnienia nie usuwa wcześniej pobranych lub odszyfrowanych kopii.
7. Podpis cyfrowy samodzielnie nie potwierdza uprawnień zawodowych lekarza.
8. Użytkownik może mieć trudności z bezpiecznym zarządzaniem kluczami.

## 4.8. Główne ograniczenie GPG

GPG odpowiada na pytanie:

> Czy użytkownik posiada odpowiedni klucz umożliwiający odszyfrowanie dokumentu?

Nie odpowiada samodzielnie na pytania:

> Czy użytkownik jest lekarzem?

> Czy lekarz może wystawić dokument dla określonego pacjenta?

> Czy pacjent może zmienić oficjalną historię dokumentacji medycznej?

Takie reguły muszą zostać zaimplementowane w warstwie aplikacji.

---

# 5. Porównanie UCAN i GPG

| Kryterium | UCAN | GPG |
|---|---|---|
| Główny cel | Autoryzacja | Szyfrowanie i podpisy |
| Brak centralnego serwera | Możliwy | Możliwy |
| Szyfrowanie dokumentów | Nie | Tak |
| Poufność dokumentów w IPFS | Nie samodzielnie | Tak |
| Delegowanie uprawnień | Tak | Nie w modelu UCAN |
| Rozdzielenie operacji odczytu i zapisu | Tak, poprzez reguły aplikacji | Wymaga osobnych reguł aplikacji |
| Ograniczenie czasowe uprawnień | Tak | Brak gwarantowanego czasowego ograniczenia odszyfrowania |
| Podpisy kryptograficzne | Tak, do autoryzacji | Tak, również do podpisywania dokumentów |
| Ochrona przed odczytem przez innego pacjenta | Wymaga szyfrowania | Tak, przy poprawnym szyfrowaniu |
| Zarządzanie kluczami | Wymagane | Wymagane |
| Integracja z IPFS | Warstwa autoryzacji | Warstwa szyfrowania |
| Złożoność minimalnego prototypu | Średnia lub wysoka | Niska lub średnia |

---

# 6. Problem rozdzielenia ról pacjenta i placówki

W projekcie zakłada się dwie podstawowe role:

1. Pacjent.
2. Placówka medyczna / lekarz.

Role te powinny posiadać różne uprawnienia.

## 6.1. Proponowana macierz uprawnień

| Operacja | Pacjent | Placówka / lekarz |
|---|---|---|
| Odczyt własnej dokumentacji | Tak | Zależnie od uprawnień |
| Odczyt cudzej dokumentacji | Nie | Tylko z odpowiednim dostępem |
| Dodanie własnego pliku | Tak | Tak |
| Wystawienie oficjalnego dokumentu medycznego | Nie | Tak |
| Podpisanie dokumentu własnym kluczem | Tak | Tak |
| Uznanie podpisu za podpis uprawnionego lekarza | Nie automatycznie | Po weryfikacji roli |
| Udostępnienie własnych dokumentów | Tak, zgodnie z modelem kluczy | Zgodnie z przyjętymi zasadami |
| Odszyfrowanie dokumentacji innego pacjenta | Nie | Tylko jeśli posiada odpowiedni dostęp |

Istotne jest rozróżnienie pomiędzy:

- możliwością technicznego dodania pliku do IPFS,
- możliwością uznania pliku za autentyczny dokument medyczny.

Każdy użytkownik może dodać dowolny plik do własnego węzła IPFS.

System EHR nie musi jednak traktować takiego pliku jako oficjalnego dokumentu medycznego.

Oficjalny dokument powinien posiadać możliwy do zweryfikowania podpis uprawnionego wystawcy.

## 6.2. Jak zapobiec podszywaniu się pacjenta pod lekarza?

Proponowany mechanizm:

1. Placówka lub lekarz posiada własną parę kluczy.
2. System EHR posiada informację, które klucze należą do zaufanych wystawców.
3. Dokument medyczny jest podpisywany kluczem prywatnym lekarza.
4. Aplikacja pacjenta weryfikuje podpis.
5. Dokument jest oznaczany jako dokument medyczny wystawiony przez uprawniony podmiot tylko wtedy, gdy podpis i uprawnienie wystawcy zostaną poprawnie zweryfikowane.

Bez centralnego serwera nadal trzeba zdefiniować zasady ustanawiania i aktualizowania zaufania do kluczy.

---

# 7. Rozważane warianty architektury

## 7.1. Wariant A – IPFS + UCAN

Architektura:

```text
Pacjent
   |
   | Delegacja UCAN
   v
Lekarz
   |
   | Żądanie dostępu
   v
Aplikacja EHR
   |
   | Weryfikacja tokenu
   v
IPFS
```

### Zalety

- precyzyjne definiowanie uprawnień,
- delegowanie dostępu,
- możliwość ograniczania uprawnień w czasie,
- zdecentralizowana weryfikacja autoryzacji.

### Wady

- brak ochrony zawartości plików bez dodatkowego szyfrowania,
- możliwość ominięcia aplikacyjnej kontroli dostępu przez bezpośredni odczyt z publicznego IPFS,
- wyższa złożoność implementacji.

### Ocena

UCAN samodzielnie nie spełnia wymagań poufności dokumentacji medycznej.

---

## 7.2. Wariant B – IPFS + GPG

Architektura:

```text
Placówka medyczna
        |
        v
Utworzenie dokumentu
        |
        v
Podpis cyfrowy lekarza
        |
        v
Szyfrowanie GPG
        |
        v
Zapis do IPFS
        |
        v
        CID
        |
        v
Pacjent pobiera szyfrogram
        |
        v
Odszyfrowanie kluczem prywatnym
        |
        v
Weryfikacja podpisu dokumentu
```

### Zalety

- zapewnienie poufności dokumentów,
- prosta integracja z istniejącym IPFS,
- możliwość szyfrowania dla wielu odbiorców,
- możliwość weryfikacji podpisów dokumentów,
- brak konieczności centralnego serwera.

### Wady

- konieczność opracowania własnej logiki ról,
- problem dystrybucji i weryfikacji kluczy,
- ograniczone możliwości cofania dostępu,
- brak natywnego delegowania szczegółowych uprawnień.

### Ocena

Wariant odpowiedni do pierwszej demonstracyjnej implementacji, której podstawowym wymaganiem jest zapewnienie poufności plików medycznych.

---

## 7.3. Wariant C – IPFS + UCAN + GPG

Architektura:

```text
                 Aplikacja EHR
                       |
             +---------+---------+
             |                   |
             v                   v
           UCAN                 GPG
             |                   |
       Autoryzacja          Szyfrowanie
       Delegowanie           Podpisy
             |                   |
             +---------+---------+
                       |
                       v
                      IPFS
```

### Zalety

- oddzielenie autoryzacji od poufności,
- możliwość delegowania szczegółowych uprawnień,
- kryptograficzna ochrona dokumentów,
- możliwość tworzenia zaawansowanego modelu ról.

### Wady

- większa złożoność architektury,
- konieczność zarządzania dwoma mechanizmami,
- trudniejsze testowanie,
- potrzeba rozwiązania problemu synchronizacji uprawnień z dostępem do kluczy szyfrujących.

### Ocena

Wariant potencjalnie najbardziej rozbudowany funkcjonalnie, ale może przekraczać zakres demonstracyjnej pracy inżynierskiej.

---

# 8. Rekomendowane rozwiązanie

Na obecnym etapie projektu rekomendowany jest wariant:

## IPFS + GPG + lokalna weryfikacja ról i podpisów

Uzasadnienie:

1. Podstawowym wymaganiem jest ochrona dokumentacji przed odczytem przez nieuprawnionych pacjentów.
2. GPG bezpośrednio realizuje wymaganie poufności.
3. IPFS pozostaje odpowiedzialny za rozproszone przechowywanie i wymianę zaszyfrowanych plików.
4. Podpisy cyfrowe mogą służyć do weryfikacji wystawcy dokumentu.
5. Lokalna logika aplikacji może rozróżniać dokumenty pacjenta od dokumentów wystawionych przez placówkę.
6. Implementacja nie wymaga pełnego systemu delegowania uprawnień.
7. Rozwiązanie jest zgodne z demonstracyjnym charakterem pracy dyplomowej.

UCAN pozostaje możliwym rozszerzeniem architektury o delegowanie uprawnień w przyszłości.

## 8.1. Zakres bezpieczeństwa proponowanego MVP

Implementacja powinna zapewniać:

- generowanie lub import par kluczy GPG,
- weryfikację kluczy publicznych uczestników,
- szyfrowanie dokumentów przed zapisem do IPFS,
- dodawanie szyfrogramów do IPFS,
- pobieranie plików na podstawie CID,
- odszyfrowywanie dokumentów przez uprawnionych odbiorców,
- podpisywanie dokumentów przez wystawcę,
- weryfikację podpisów,
- rozróżnienie roli pacjenta i placówki na poziomie aplikacji.

Nie zakłada się implementacji pełnej infrastruktury zarządzania tożsamością i uprawnieniami.

---

# 9. Proponowane scenariusze testowe

| ID | Scenariusz | Oczekiwany rezultat |
|---|---|---|
| T01 | Placówka szyfruje dokument dla pacjenta A | Powstaje poprawny szyfrogram |
| T02 | Placówka dodaje szyfrogram do IPFS | IPFS zwraca CID |
| T03 | Pacjent A pobiera dokument z IPFS | Plik zostaje pobrany |
| T04 | Pacjent A odszyfrowuje dokument | Odszyfrowanie kończy się powodzeniem |
| T05 | Pacjent B pobiera szyfrogram pacjenta A | Pobranie może się udać |
| T06 | Pacjent B próbuje odszyfrować dokument pacjenta A | Odszyfrowanie kończy się niepowodzeniem |
| T07 | Lekarz podpisuje dokument, a pacjent weryfikuje podpis | Podpis zostaje poprawnie zweryfikowany |
| T08 | Pacjent próbuje przedstawić własny podpis jako podpis lekarza | Aplikacja nie uznaje dokumentu za wystawiony przez zaufanego lekarza |
| T09 | Zaszyfrowany dokument zostaje zmodyfikowany | Zmiana CID lub błąd weryfikacji integralności wykrywa modyfikację |
| T10 | Dokument jest pobierany z drugiego węzła IPFS | Transfer pomiędzy niezależnymi węzłami przebiega poprawnie |

Testy powinny wykorzystywać syntetyczne dane medyczne.

---

# 10. Ograniczenia proponowanego rozwiązania

## 10.1. Cofnięcie dostępu

Jeśli lekarz uzyskał dostęp do dokumentu, odszyfrował go i zapisał kopię, późniejsze cofnięcie uprawnienia nie usuwa tej kopii.

Dotyczy to zarówno architektury opartej na GPG, jak i modelu UCAN połączonego z szyfrowaniem.

Dostęp do przyszłych wersji dokumentów można ograniczyć poprzez zastosowanie nowych kluczy i odpowiednie ponowne szyfrowanie.

## 10.2. Utrata klucza prywatnego

Utrata klucza prywatnego może uniemożliwić odszyfrowanie dokumentacji, jeśli nie istnieje odpowiedni mechanizm odzyskiwania dostępu.

System powinien przewidywać przynajmniej procedurę tworzenia bezpiecznej kopii zapasowej kluczy na potrzeby demonstracji.

## 10.3. Zarządzanie zaufaniem

System musi wiedzieć, które klucze publiczne reprezentują uprawnione placówki i lekarzy.

Możliwe podejścia obejmują:

- lokalną listę zaufanych kluczy,
- ręczną weryfikację kluczy,
- podpisywanie kluczy przez zaufane podmioty,
- dodatkowy zdecentralizowany mechanizm potwierdzania tożsamości.

Wybór konkretnego mechanizmu wymaga odrębnej decyzji architektonicznej.

## 10.4. Dostępność dokumentów

IPFS nie gwarantuje stałego przechowywania dokumentów.

Węzły odpowiedzialne za utrzymywanie dokumentacji powinny stosować mechanizm pinning lub inną strategię replikacji.

## 10.5. Prywatność metadanych

Szyfrowanie pliku chroni jego zawartość, ale niekoniecznie wszystkie metadane związane z jego przechowywaniem, identyfikatorami i wymianą w sieci IPFS.

Nie należy publikować w jawnej postaci:

- imion i nazwisk pacjentów,
- numerów PESEL,
- rozpoznań medycznych,
- identyfikatorów pozwalających bezpośrednio powiązać dokument z konkretną osobą.

W demonstracyjnym prototypie należy wykorzystywać dane fikcyjne.

## 10.6. Integralność i autentyczność

CID umożliwia identyfikowanie zawartości.

Nie potwierdza jednak samodzielnie, że dany dokument został wystawiony przez uprawnionego lekarza.

Do potwierdzania kryptograficznego pochodzenia dokumentów należy wykorzystać podpisy cyfrowe i wiarygodne powiązanie kluczy z rolami uczestników.

---

# 11. Otwarte decyzje architektoniczne

Przed implementacją wymagają ustalenia następujące kwestie.

### D01. Zarządzanie kluczami

- Czy aplikacja będzie generowała klucze GPG?
- Czy użytkownicy będą importowali istniejące klucze?
- Gdzie będą przechowywane klucze prywatne?
- Jak będą weryfikowane klucze publiczne?

### D02. Model uprawnień

- Czy lekarz ma mieć stały dostęp do dokumentacji pacjenta?
- Czy pacjent musi każdorazowo wyrażać zgodę na udostępnienie?
- Czy placówka ma zachowywać własną kopię wystawionych dokumentów?
- Czy rozdzielamy rolę placówki i indywidualnego lekarza?

### D03. Przechowywanie dokumentów

- Który węzeł odpowiada za pinning?
- Czy pliki są przechowywane jednocześnie na węźle pacjenta i placówki?
- Jak aplikacja przechowuje powiązania pomiędzy pacjentem, dokumentem a CID?
- Jak zapewnić dostępność dokumentacji po wyłączeniu jednego węzła?

### D04. Podpisywanie dokumentów

- Kto podpisuje dokument: lekarz czy placówka?
- Jak aplikacja weryfikuje uprawnienia wystawcy?
- Czy dopuszczamy podpisywanie dokumentów przez pacjenta?
- Jak rozróżniać oficjalne dokumenty medyczne od prywatnych plików?

### D05. Udostępnianie dokumentów

- Czy pacjent może samodzielnie udostępnić dokument innej placówce?
- Czy udostępnienie wymaga ponownego szyfrowania?
- Czy dostęp ma być ograniczony czasowo?
- Czy w przyszłości będzie wykorzystywany UCAN?

---

# 12. Wnioski końcowe

UCAN i GPG rozwiązują różne problemy bezpieczeństwa.

**UCAN** jest mechanizmem zdecentralizowanej autoryzacji. Umożliwia nadawanie i delegowanie uprawnień, lecz nie szyfruje dokumentów przechowywanych w IPFS.

**GPG** jest mechanizmem szyfrowania i podpisywania danych. Umożliwia ochronę poufności dokumentów oraz kryptograficzną weryfikację ich pochodzenia, lecz nie zapewnia kompletnego modelu uprawnień EHR.

Dla demonstracyjnej implementacji systemu EHR rekomendowane jest zastosowanie:

**IPFS + GPG + lokalna logika ról i weryfikacja podpisów cyfrowych.**

Rozwiązanie pozwala:

1. Zachować zdecentralizowane przechowywanie i wymianę dokumentów.
2. Chronić dokumenty przed odszyfrowaniem przez innych pacjentów.
3. Podpisywać dokumenty wystawiane przez lekarzy.
4. Weryfikować kryptograficzne pochodzenie dokumentów.
5. Rozróżniać role pacjenta i placówki medycznej.
6. Przygotować działającą implementację demonstracyjną bez rozbudowanej infrastruktury centralnej.

Należy jednocześnie pamiętać, że decentralizacja infrastruktury nie usuwa potrzeby ustanowienia zasad zaufania i identyfikacji uprawnionych wystawców dokumentów.

UCAN może zostać wykorzystany na dalszym etapie projektu jako dodatkowa warstwa zarządzania delegacjami uprawnień.

---

# 13. Literatura i dokumentacja techniczna

## 13.1. IPFS

1. Benet J., *IPFS – Content Addressed, Versioned, P2P File System*, 2014.  
   https://arxiv.org/abs/1407.3561

2. IPFS Documentation – Concepts.  
   https://docs.ipfs.tech/concepts/

3. IPFS Documentation – Privacy and Encryption.  
   https://docs.ipfs.tech/concepts/privacy-and-encryption/

4. IPFS Documentation – Persistence.  
   https://docs.ipfs.tech/how-to/pin-files/

## 13.2. UCAN

5. UCAN Working Group – UCAN Specification.  
   https://github.com/ucan-wg/spec

6. UCAN Working Group – Delegation Specification.  
   https://github.com/ucan-wg/delegation

## 13.3. GPG / OpenPGP

7. GnuPG – Official Documentation.  
   https://gnupg.org/documentation/

8. OpenPGP Message Format – RFC 9580.  
   https://www.rfc-editor.org/rfc/rfc9580.html

## 13.4. EHR i IPFS

9. Reen G. S., Mohandas M., Venkatesan S., *Decentralized Patient Centric e-Health Record Management System using Blockchain and IPFS*, 2020.  
   https://arxiv.org/abs/2009.14285

---

## Status decyzji

| Element | Stan |
|---|---|
| Protokół rozproszonego przechowywania | IPFS |
| Mechanizm poufności | GPG – wstępnie rekomendowany |
| Mechanizm podpisów | GPG / OpenPGP – wstępnie rekomendowany |
| Rozdzielenie ról | Lokalna logika aplikacji i zaufane klucze |
| Delegowanie uprawnień UCAN | Opcjonalne rozszerzenie |
| Centralny serwer autoryzacji | Nieprzewidziany |
| Centralna baza dokumentów | Nieprzewidziana |
| Liczba węzłów do demonstracji | Minimum 2 |
| Rodzaj danych testowych | Syntetyczne dane medyczne |
| Ostateczna architektura | Do zatwierdzenia przed implementacją |