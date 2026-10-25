IPLD (InterPlanetary Linked Data) to ustandaryzowana warstwa modelu danych w ekosystemie IPFS, która łączy różne struktury informacji za pomocą kryptograficznych skrótów (hashy) w jeden globalny graf skierowany (Merkle DAG). Działa jako uniwersalny tłumacz – sprawia, że system może w jednolity sposób adresować (poprzez CID) i przemieszczać przeróżne formaty danych, traktując pliki z UnixFS, obiekty z repozytoriów Git, czy bloki z Ethereum dokładnie tak samo: jako powiązane ze sobą węzły z unikalnym adresem.
Boxo Blockstore (Implementacja referencyjna w Go): W oficjalnym węźle Kubo (wcześniej go-ipfs), cały silnik i interfejsy Blockstore zostały niedawno wydzielone do biblioteki Boxo. To tam znajduje się produkcyjny kod metod takich jak Get, Put, Has, Delete oraz implementacje magazynów pamięciowych i dyskowych.

    Kod źródłowy: [https://github.com/ipfs/boxo/tree/main/blockstore](https://github.com/ipfs/boxo/tree/main/blockstore)

    Datastores (Warstwa poniżej Blockstore): W architekturze IPFS, Blockstore rzadko sam pisze bity na dysk – zazwyczaj deleguje to do niższej warstwy zwanej Datastore (często wykorzystującej systemy takie jak flatfs, badger czy leveldb).

    Interfejsy Datastore: [https://github.com/ipfs/go-datastore](https://github.com/ipfs/go-datastore)

    System plików płaskich (FlatFS) używany domyślnie przez Kubo: [https://github.com/ipfs/go-ds-flatfs](https://github.com/ipfs/go-ds-flatfs)

    Specyfikacja IPLD i IPFS: Oficjalna strona ze specyfikacjami (Specs) opisuje, jak z surowych bloków (Raw Blocks) budowane są struktury logiczne.

    Specyfikacja bloków: [https://ipld.io/specs/codecs/raw/](https://ipld.io/specs/codecs/raw/)

    Ogólna dokumentacja bloków IPFS: [https://docs.ipfs.tech/concepts/how-ipfs-works/#directed-acyclic-graphs-dags](https://docs.ipfs.tech/concepts/how-ipfs-works/#directed-acyclic-graphs-dags)
    Rust-IPFS (Implementacje w języku Rust): Mimo że oficjalną referencją jest Go, społeczność stworzyła solidne fundamenty w Ryscie, na których warto się wzorować przy projektowaniu własnego Blockstore w ramach pracy inżynierskiej.

    Repozytorium rust-ipfs i ich zarządzanie repozytorium/blokami: [https://github.com/rs-ipfs/rust-ipfs/tree/master/src/repo](https://github.com/rs-ipfs/rust-ipfs/tree/master/src/repo)


Interfejs Datastore i jego implementacja FilesystemDatastore.



Blockstore korzystający z ipfs-cid.



put_block(cid, data), get(cid), has(cid).



Jednoznaczne mapowanie CID na klucz dyskowy.



Atomowy zapis bez nadpisywania poprawnego bloku.



Kontrola integralności przy zapisie i odczycie.



Testy trwałości, deduplikacji, korupcji i błędnych danych.

W projekcie zastosowano dwuwarstwową architekturę magazynowania bloków, wzorowaną na podziale Blockstore/Datastore znanym z ekosystemu IPFS. Blockstore odpowiada za operacje na blokach identyfikowanych przez CID oraz kontrolę integralności danych, natomiast Filesystem Datastore zapewnia trwałe przechowywanie bajtów w lokalnym systemie plików. Rozwiązanie celowo pomija zaawansowane optymalizacje produkcyjne, zachowując kluczowe właściwości trwałości, deduplikacji i adresowania zawartością.

LevelDB is a fast, open-source, embedded key-value storage library written in C++ by Google engineers Jeff Dean and Sanjay Ghemawat.

Do zapisywania bajtow plikow po kluczu CID na dysku urzadzenia bedzie wykorzystana biblioteka LevelDB
LevelDB - C++ opencource Google Library
Multicodec (raw, dag-pb itd.) nie jest częścią klucza. Dwa CID różniące się kodekiem, ale zawierające ten sam multihash, mogą wskazywać jeden zapisany blok.
