//! CID wzorcowe pochodzą z Kubo 0.43.1:
//! `ipfs add -Q -n --cid-version=1 --raw-leaves --chunker=<chunker> <plik>`.
use ipfs_cid::consts::{CODEC_DAG_PB, CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;
use ipfs_store::PersistentBlockstore;
use ipfs_unixfs::consts::MAX_EXPORT_BYTES;
use ipfs_unixfs::models::{DataType, PbLink, PbNode, UnixfsData};
use ipfs_unixfs::{Chunker, UnixfsError, add_file, block_links, cat, dag_cids};
use prost::Message;
use std::path::Path;

/// Te same dane co w skrypcie, którym policzono CID w Kubo.
fn pseudo_random(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state = (state * 1_103_515_245 + 12_345) & 0x7fff_ffff;
            (state >> 16) as u8
        })
        .collect()
}

fn patient_text() -> Vec<u8> {
    (0..30_000u32)
        .flat_map(|i| {
            format!(
                "Patient {}: observation {}, value {} mmHg\n",
                i % 97,
                i,
                (i * 37) % 200
            )
            .into_bytes()
        })
        .collect()
}

fn store() -> (tempfile::TempDir, PersistentBlockstore) {
    let directory = tempfile::tempdir().unwrap();
    let store = PersistentBlockstore::open_in(directory.path()).unwrap();
    (directory, store)
}

fn chunks(chunker: Chunker, data: &[u8]) -> Vec<Vec<u8>> {
    chunker.split(data).unwrap().map(Result::unwrap).collect()
}

fn read_back(store: &PersistentBlockstore, root: &Cid) -> Vec<u8> {
    let mut output = Vec::new();
    cat(store, root, &mut output).unwrap();
    output
}

fn small_rabin() -> Chunker {
    Chunker::Rabin {
        min: 2048,
        avg: 4096,
        max: 8192,
    }
}

#[test]
fn fixed_chunks_have_no_padding() {
    let data = pseudo_random(2500, 1);
    let parts = chunks(Chunker::Fixed { size: 1024 }, &data);
    assert_eq!(
        parts.iter().map(Vec::len).collect::<Vec<_>>(),
        [1024, 1024, 452]
    );
    assert_eq!(parts.concat(), data);
    assert!(chunks(Chunker::Fixed { size: 1024 }, &[]).is_empty());
}

#[test]
fn rabin_chunks_respect_bounds_and_cover_the_file() {
    let data = pseudo_random(300_000, 3);
    let parts = chunks(small_rabin(), &data);
    assert!(parts.len() > 20);
    let (last, full) = parts.split_last().unwrap();
    assert!(full.iter().all(|part| (2048..=8192).contains(&part.len())));
    assert!(last.len() <= 8192);
    // Granice zależą od treści: nie wszystkie kawałki mają tę samą długość.
    assert!(full.iter().any(|part| part.len() != full[0].len()));
    assert_eq!(parts.concat(), data);
}

#[test]
fn invalid_chunker_parameters_are_rejected() {
    for chunker in [
        Chunker::Fixed { size: 0 },
        Chunker::Fixed {
            size: 3 * 1024 * 1024,
        },
        Chunker::Rabin {
            min: 8,
            avg: 64,
            max: 128,
        },
        Chunker::Rabin {
            min: 64,
            avg: 64,
            max: 128,
        },
        Chunker::Rabin {
            min: 64,
            avg: 128,
            max: 128,
        },
        Chunker::Rabin {
            min: 64,
            avg: 128,
            max: 3 * 1024 * 1024,
        },
    ] {
        assert!(matches!(
            chunker.split(&b""[..]),
            Err(UnixfsError::InvalidChunker(_))
        ));
    }
}

#[test]
fn audio_and_video_use_fixed_chunks_everything_else_rabin() {
    for name in ["ekg.MP4", "/data/heart.wav", "scan.mkv"] {
        assert_eq!(Chunker::for_path(Path::new(name)), Chunker::fixed());
    }
    for name in ["record.json", "notes.txt", "xray.bmp", "no-extension"] {
        assert_eq!(Chunker::for_path(Path::new(name)), Chunker::rabin());
    }
    assert_eq!(Chunker::fixed().kubo_name(), "size-262144");
    assert_eq!(Chunker::rabin().kubo_name(), "rabin-16384-65536-262144");
}

#[test]
fn small_file_is_a_single_raw_block() {
    let (_directory, store) = store();
    let data = b"small EHR note";
    for chunker in [Chunker::fixed(), Chunker::rabin()] {
        let imported = add_file(&store, &data[..], chunker).unwrap();
        assert_eq!(
            imported.root,
            Cid::generate_from_data(data, CODEC_RAW, SHA2_256).unwrap()
        );
        assert_eq!(imported.size, data.len() as u64);
        assert_eq!(read_back(&store, &imported.root), data);
    }
}

#[test]
fn empty_file_matches_kubo() {
    let (_directory, store) = store();
    let imported = add_file(&store, &b""[..], Chunker::rabin()).unwrap();
    assert_eq!(
        imported.root.to_string_base32(),
        "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"
    );
    assert!(read_back(&store, &imported.root).is_empty());
}

/// 391 liści po 1024 B: korzeń ma 3 węzły pośrednie (174 + 174 + 43), czyli dwa poziomy.
#[test]
fn balanced_two_level_dag_matches_kubo() {
    let (_directory, store) = store();
    let data = pseudo_random(400_000, 1);
    let imported = add_file(&store, data.as_slice(), Chunker::Fixed { size: 1024 }).unwrap();
    assert_eq!(
        imported.root.to_string_base32(),
        "bafybeialngtx5kfztjrhfccvg6psujrcc3knsyzd43ofpe4tozghpbvwrm"
    );
    assert_eq!(imported.root.codec, CODEC_DAG_PB);
    assert_eq!(imported.size, 400_000);
    assert_eq!(imported.new_blocks, 391 + 3 + 1);
    assert_eq!(dag_cids(&store, &imported.root).unwrap().len(), 395);
    assert_eq!(read_back(&store, &imported.root), data);
}

#[test]
fn fixed_default_chunker_matches_kubo() {
    let (_directory, store) = store();
    let data = pseudo_random(1_000_000, 7);
    let imported = add_file(&store, data.as_slice(), Chunker::fixed()).unwrap();
    assert_eq!(
        imported.root.to_string_base32(),
        "bafybeif5sfe36czajhwln6al3vs5ulvvednungw572fha4zyfw7k6t4uoe"
    );
    assert_eq!(read_back(&store, &imported.root), data);
}

#[test]
fn rabin_chunker_matches_kubo() {
    let (_directory, store) = store();
    let binary = pseudo_random(1_000_000, 7);
    let text = patient_text();
    let text_rabin = Chunker::Rabin {
        min: 1024,
        avg: 3000,
        max: 6000,
    };
    for (data, chunker, kubo_cid) in [
        (
            &binary,
            Chunker::rabin(),
            "bafybeiboymxpj7o3mceb2xukvtjl5ilssmse2wznc5cf5ro6cwizl5j5iy",
        ),
        (
            &text,
            Chunker::rabin(),
            "bafybeicbi7idsqjkq3skv7utx5gu2xq7fgbmufynnqbxrmah7ir35375ea",
        ),
        // Domyślne parametry `--chunker=rabin` w Kubo.
        (
            &binary,
            Chunker::Rabin {
                min: 87_381,
                avg: 262_144,
                max: 393_216,
            },
            "bafybeic2bjuaxvokxtgygoqpoq7zij6c6hkfivtvdpyjqsljq46tt5476m",
        ),
        (
            &binary,
            small_rabin(),
            "bafybeigilmw4hfehsszdgnm7ej3efjpk2umjvcmbaredsgk4fwbo62wh7e",
        ),
        (
            &text,
            text_rabin,
            "bafybeifiadcns2uzsc447fsedy34gpc6goqnrunhyyjzbgkmosj26acryy",
        ),
    ] {
        let imported = add_file(&store, data.as_slice(), chunker).unwrap();
        assert_eq!(imported.root.to_string_base32(), kubo_cid);
        assert_eq!(&read_back(&store, &imported.root), data);
    }
}

/// Wstawka na początku pliku przesuwa wszystkie stałe kawałki, a Rabin odnajduje
/// stare granice: większość bloków nowej wersji już jest w Blockstore.
#[test]
fn rabin_reuses_blocks_after_an_insertion_and_fixed_does_not() {
    let original = pseudo_random(300_000, 5);
    let mut edited = b"new header line\n".to_vec();
    edited.extend_from_slice(&original);

    let (_directory, store) = store();
    let first = add_file(&store, original.as_slice(), small_rabin()).unwrap();
    assert_eq!(first.reused_blocks, 0);
    let second = add_file(&store, edited.as_slice(), small_rabin()).unwrap();
    assert!(
        second.reused_blocks * 10 >= first.new_blocks * 9,
        "reused {} of {}",
        second.reused_blocks,
        first.new_blocks
    );
    assert_eq!(read_back(&store, &second.root), edited);
    // Ten sam plik dodany drugi raz nie zapisuje niczego.
    assert_eq!(
        add_file(&store, original.as_slice(), small_rabin())
            .unwrap()
            .new_blocks,
        0
    );

    let (_directory, store) = self::store();
    let fixed = Chunker::Fixed { size: 4096 };
    add_file(&store, original.as_slice(), fixed).unwrap();
    assert_eq!(
        add_file(&store, edited.as_slice(), fixed)
            .unwrap()
            .reused_blocks,
        0
    );
}

fn unixfs_node(data_type: DataType, data: Option<&[u8]>, links: &[&Cid]) -> Vec<u8> {
    let unixfs = UnixfsData {
        r#type: data_type as i32,
        data: data.map(<[u8]>::to_vec),
        filesize: None,
        blocksizes: Vec::new(),
    };
    PbNode {
        data: Some(unixfs.encode_to_vec()),
        links: links
            .iter()
            .map(|cid| PbLink {
                hash: Some(cid.to_bytes()),
                name: Some(String::new()),
                tsize: None,
            })
            .collect(),
    }
    .to_canonical_bytes()
}

/// Węzeł pliku z rozmiarami deklarowanymi jak w Kubo: `filesize` i `blocksizes` dzieci.
fn sized_node(links: &[(Vec<u8>, u64)], filesize: u64) -> Vec<u8> {
    let unixfs = UnixfsData {
        r#type: DataType::File as i32,
        data: None,
        filesize: Some(filesize),
        blocksizes: links.iter().map(|(_, size)| *size).collect(),
    };
    PbNode {
        data: Some(unixfs.encode_to_vec()),
        links: links
            .iter()
            .map(|(hash, _)| PbLink {
                hash: Some(hash.clone()),
                name: Some(String::new()),
                tsize: None,
            })
            .collect(),
    }
    .to_canonical_bytes()
}

/// Kubo bez `--raw-leaves` trzyma dane liścia w polu Data węzła dag-pb i linkuje CIDv0.
#[test]
fn reads_dag_pb_leaves_and_cid_v0_links() {
    let (_directory, store) = store();
    let leaf_a = store
        .put_with_codec(
            &unixfs_node(DataType::File, Some(b"first "), &[]),
            CODEC_DAG_PB,
            SHA2_256,
        )
        .unwrap();
    let leaf_b = store
        .put_with_codec(
            &unixfs_node(DataType::Raw, Some(b"second"), &[]),
            CODEC_DAG_PB,
            SHA2_256,
        )
        .unwrap();
    // CIDv0 to sam multihash: 0x12 0x20 + skrót.
    let mut v0_link = vec![0x12, 0x20];
    v0_link.extend_from_slice(&leaf_a.multihash.digest);
    let root_bytes = sized_node(&[(v0_link, 6), (leaf_b.to_bytes(), 6)], 12);
    let root_cid = store
        .put_with_codec(&root_bytes, CODEC_DAG_PB, SHA2_256)
        .unwrap();

    assert_eq!(read_back(&store, &root_cid), b"first second");
    assert_eq!(
        block_links(&root_cid, &root_bytes).unwrap(),
        [leaf_a, leaf_b]
    );
}

#[test]
fn dag_pb_links_are_encoded_before_data() {
    let leaf = Cid::generate_from_data(b"x", CODEC_RAW, SHA2_256).unwrap();
    let bytes = unixfs_node(DataType::File, None, &[&leaf]);
    // 0x12 = pole 2 (Links), 0x0a = pole 1 (Data).
    assert_eq!(bytes[0], 0x12);
    assert_eq!(bytes[bytes.len() - 4..], [0x0a, 0x02, 0x08, 0x02]);
}

#[test]
fn unsupported_and_incomplete_dags_are_errors() {
    let (_directory, store) = store();
    let directory = store
        .put_with_codec(
            &unixfs_node(DataType::Directory, None, &[]),
            CODEC_DAG_PB,
            SHA2_256,
        )
        .unwrap();
    assert!(matches!(
        cat(&store, &directory, &mut Vec::new()),
        Err(UnixfsError::Unsupported { .. })
    ));

    let missing = Cid::generate_from_data(b"never stored", CODEC_RAW, SHA2_256).unwrap();
    let root = store
        .put_with_codec(
            &sized_node(&[(missing.to_bytes(), 12)], 12),
            CODEC_DAG_PB,
            SHA2_256,
        )
        .unwrap();
    assert!(matches!(
        cat(&store, &root, &mut Vec::new()),
        Err(UnixfsError::MissingBlock(_))
    ));
    assert!(matches!(
        dag_cids(&store, &root),
        Err(UnixfsError::MissingBlock(_))
    ));

    let garbage = store
        .put_with_codec(&[0xff, 0xff, 0xff], CODEC_DAG_PB, SHA2_256)
        .unwrap();
    assert!(matches!(
        cat(&store, &garbage, &mut Vec::new()),
        Err(UnixfsError::Decode(_))
    ));
}

/// Spreparowane DAG-i: deklaracje rozmiarów muszą się zgadzać, a `cat` nie może zapisać
/// więcej, niż deklaruje korzeń (ani więcej niż `MAX_EXPORT_BYTES`).
#[test]
fn crafted_dags_are_rejected_before_they_grow_the_output() {
    let (_directory, store) = store();
    let leaf = store.put_with_codec(b"x", CODEC_RAW, SHA2_256).unwrap();
    let link = || leaf.to_bytes();
    let put = |bytes: Vec<u8>| {
        store
            .put_with_codec(&bytes, CODEC_DAG_PB, SHA2_256)
            .unwrap()
    };
    let cat_error = |root: &Cid| {
        let mut output = Vec::new();
        let error = cat(&store, root, &mut output).unwrap_err();
        (error, output)
    };

    // 1000 linków do tego samego bloku przy deklaracji 1 B: suma nie zgadza się z filesize.
    let amplified = put(sized_node(&vec![(link(), 1); 1000], 1));
    let (error, output) = cat_error(&amplified);
    assert!(matches!(error, UnixfsError::InvalidDag { .. }), "{error}");
    assert!(output.is_empty());

    // Rodzic deklaruje dziecku 5 B, a blok ma 1 B.
    let lying_parent = put(sized_node(&[(link(), 5)], 5));
    assert!(matches!(
        cat_error(&lying_parent).0,
        UnixfsError::InvalidDag { .. }
    ));

    // Dziecko o rozmiarze zero (amplifikacja bez bajtów).
    let zero_child = put(sized_node(&[(link(), 0)], 0));
    assert!(matches!(
        cat_error(&zero_child).0,
        UnixfsError::InvalidDag { .. }
    ));

    // Korzeń deklaruje więcej, niż dozwolone: odrzucony przed zapisem czegokolwiek.
    let too_large = MAX_EXPORT_BYTES + 1;
    let huge = put(sized_node(&[(link(), too_large)], too_large));
    let (error, output) = cat_error(&huge);
    assert!(matches!(error, UnixfsError::FileTooLarge { .. }), "{error}");
    assert!(output.is_empty());

    // Uczciwy DAG z powtórzonym blokiem nadal działa (deduplikacja liści).
    let honest = put(sized_node(&[(link(), 1), (link(), 1)], 2));
    let mut output = Vec::new();
    assert_eq!(cat(&store, &honest, &mut output).unwrap(), 2);
    assert_eq!(output, b"xx");
}
