# 10 — Źródła

**Zasada autora:** dokumentacja IPFS (docs.ipfs.tech, specs.ipfs.tech) i specyfikacje libp2p są głównym źródłem wiedzy i weryfikacji. Artykuły naukowe służą do części analitycznej. Reddit, fora i YouTube nie nadają się do cytowania, tylko do orientacji.
(Usunięto duplikaty i parametry `utm_source`. Linki do `pdf.sciencedirectassets.com` z tokenem wygasają, więc używaj adresów DOI/artykułu.)

## Literatura z karty pracy
- Benet J., *IPFS – Content Addressed, Versioned, P2P File System*, 2014 — https://arxiv.org/abs/1407.3561
- Reen, Mohandas, Venkatesan, *Decentralized Patient Centric e-Health Record Management System using Blockchain and IPFS*, 2020 — https://arxiv.org/abs/2009.14285
- Guasch Repullés M., *IPFS as a Foundation for Anonymous File Storage*, UOC — https://openaccess.uoc.edu/server/api/core/bitstreams/61c176f3-f596-480c-ad4f-8b3cbc29ef4f/content
- IPFS Docs, IPNS / Mutability — https://docs.ipfs.tech/concepts/ipns/#mutability-in-ipfs

## Specyfikacje i dokumentacja IPFS / IPLD / multiformats
- Principles (wymagania wobec implementacji) — https://specs.ipfs.tech/architecture/principles/
- CID — https://specs.ipfs.tech/cid/ · https://github.com/ipfs/specs/blob/main/src/cid.md · https://cid.ipfs.tech/ · https://docs.ipfs.tech/concepts/content-addressing/ (też #cid-conversion) · https://github.com/ipfs/specs/issues/130
- Multibase — https://multiformats.readthedocs.io/en/latest/multibase.html · multicodec https://github.com/multiformats/multicodec · rust-multihash https://github.com/multiformats/rust-multihash
- Merkle DAG — https://docs.ipfs.tech/concepts/merkle-dag/ · https://dag.ipfs.tech/
- Lifecycle — https://docs.ipfs.tech/concepts/lifecycle/
- UnixFS — https://specs.ipfs.tech/unixfs/
- IPLD — https://ipld.io/docs/intro/primer/ · https://ipld.io/docs/codecs/known/ · https://ipld.io/docs/data-model/node/ · https://ipld.io/specs/codecs/raw/ · CAR https://ipld.io/specs/transport/car/carv1/ · encryption https://ipld.io/docs/synthesis/encryption/ · https://github.com/ipld/docs
- Bitswap — https://specs.ipfs.tech/bitswap-protocol/ · (historyczne) https://github.com/ipfs/js-ipfs-bitswap
- Kademlia DHT — https://specs.ipfs.tech/routing/kad-dht/ · https://github.com/libp2p/specs/blob/master/kad-dht/README.md · https://docs.ipfs.tech/concepts/dht/ · https://libp2p.io/docs/dht/
- HTTP routing v1 — https://specs.ipfs.tech/routing/http-routing-v1/
- IPFS URI — https://specs.ipfs.tech/ipfs-uri/
- Privacy & encryption (co jest publiczne) — https://docs.ipfs.tech/concepts/privacy-and-encryption/#what-s-public-on-ipfs
- Persistence, pinning, long-term storage — https://docs.ipfs.tech/concepts/persistence/
- Klaster / infrastruktura — https://docs.ipfs.tech/install/server-infrastructure/#create-a-local-cluster
- Kubo CLI i config — https://docs.ipfs.tech/reference/kubo/cli/ · https://github.com/ipfs/kubo/blob/master/docs/config.md
- Boxo blockstore — https://github.com/ipfs/boxo/tree/main/blockstore · go-datastore https://github.com/ipfs/go-datastore · flatfs https://github.com/ipfs/go-ds-flatfs
- Awesome IPFS — https://awesome.ipfs.tech/

## libp2p / Rust
- rust-libp2p 0.57.1 — https://docs.rs/libp2p/0.57.1/libp2p/ · libp2p-kad https://docs.rs/libp2p-kad/
- libp2p concepts (bootstrap, peers, addressing, Noise) — https://docs.libp2p.io/
- Identify / Ping specs — https://github.com/libp2p/specs/tree/master/identify · https://github.com/libp2p/specs/tree/master/ping
- rusty-leveldb — https://docs.rs/rusty-leveldb/latest/rusty_leveldb/ · LevelDB impl https://github.com/google/leveldb/blob/main/doc/impl.md
- Protobuf — https://protobuf.dev/
- ipfs-only-hash (CID lokalnie) — https://github.com/alanshaw/ipfs-only-hash · https://discuss.ipfs.tech/t/how-to-calculate-cid-locally/10507
- Kademlia (orientacyjnie) — https://en.wikipedia.org/wiki/Kademlia · oryginał: Maymounkov P., Mazières D., *Kademlia: A Peer-to-peer Information System Based on the XOR Metric*, IPTPS 2002

## EHR / standardy medyczne / dane testowe
- HL7 FHIR — https://hl7.org/fhir/ · DocumentReference https://hl7.org/fhir/documentreference.html · Patient example https://build.fhir.org/patient-example.xml.html · Observation examples https://hl7.org/fhir/observation-examples.html
- Zbiory danych (syntetyczne/publiczne; wymagają zgód/credentialingu): MIMIC-CXR-JPG https://physionet.org/content/mimic-cxr-jpg/2.1.0/ · MIMIC-IV-ECG https://physionet.org/content/mimic-iv-ecg/1.0/ · NIH ChestX-ray https://nihcc.app.box.com/v/ChestXray-NIHCC · NHANES lab https://wwwn.cdc.gov/Nchs/Nhanes/ · Wikimedia Chest X-ray (licencje plików) · Open-i https://openi.nlm.nih.gov/
- RODO, prawa podmiotu danych — https://www.edpb.europa.eu/topics/key-gdpr-concepts/data-subject-rights_en

## Bezpieczeństwo
- UCAN spec — https://github.com/ucan-wg/spec · delegation https://github.com/ucan-wg/delegation
- OpenPGP RFC 9580 — https://www.rfc-editor.org/rfc/rfc9580.html · GnuPG docs https://gnupg.org/documentation/
- SimpleX (inspiracja architektoniczna, bez identyfikatorów użytkownika) — https://simplex.chat/docs/simplex.html

## Artykuły naukowe zebrane przez autora (do części analitycznej)
- https://www.sciencedirect.com/science/article/pii/S1389128623003560
- https://www.sciencedirect.com/science/article/abs/pii/S1389128624000550
- https://www.sciencedirect.com/science/article/pii/S1877050923020446
- https://link.springer.com/article/10.1007/s41019-024-00263-w
- https://www.mdpi.com/1424-8220/24/7/2197
- https://www.mdpi.com/2813-5288/3/4/12
- https://pmc.ncbi.nlm.nih.gov/articles/PMC8080150/
- https://pmc.ncbi.nlm.nih.gov/articles/PMC11461297/
- https://arxiv.org/abs/1202.4503
- https://arxiv.org/abs/2105.08395
- Ink & Switch, Peritext (CRDT, kontekst) — https://www.inkandswitch.com/peritext/ · etcd (kontrast: konsensus scentralizowany) — https://etcd.io/
