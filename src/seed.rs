//! Demo/sample data: two vet users and a realistic slice of an Indonesian
//! small-animal clinic. Idempotent — refuses to run when users already exist.
//!
//! Dates are relative to "now" so the dashboard and the daily reminder job
//! always have something to show (a vaccination due in 2 days, one overdue,
//! a low-stock vaccine, a drug expiring within 30 days, appointments today).

use chrono::{Duration, NaiveDate, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::appointments::models::AppointmentStatus;
use crate::domain::auth::password;
use crate::domain::inventory::models::{InventoryCategory, MovementType};
use crate::domain::patients::models::{PatientStatus, Sex, Species};
use crate::domain::users::models::UserRole;
use crate::error::AppError;

const DEFAULT_SEED_PASSWORD: &str = "PetCare#2026";

pub async fn run(db: &PgPool) -> Result<(), AppError> {
    let existing: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
        .fetch_one(db)
        .await?;
    if existing > 0 {
        tracing::info!("database already contains users — skipping seed");
        return Ok(());
    }

    let seed_password =
        std::env::var("SEED_PASSWORD").unwrap_or_else(|_| DEFAULT_SEED_PASSWORD.to_string());
    // argon2 runs on the blocking pool; hash once per user before the tx
    let citra_hash = password::hash_password(seed_password.clone()).await?;
    let bagus_hash = password::hash_password(seed_password.clone()).await?;

    let now = Utc::now();
    let today = now.date_naive();
    let mut tx = db.begin().await?;

    // ---- users ----
    let u_citra = Uuid::now_v7();
    let u_bagus = Uuid::now_v7();
    insert_user(
        &mut tx,
        u_citra,
        "drh. Citra Lestari",
        "citra@citrapetcare.id",
        &citra_hash,
        UserRole::Admin,
    )
    .await?;
    insert_user(
        &mut tx,
        u_bagus,
        "drh. Bagus Pramana",
        "bagus@citrapetcare.id",
        &bagus_hash,
        UserRole::Vet,
    )
    .await?;

    // ---- owners ----
    let o_ratna = Uuid::now_v7();
    let o_agus = Uuid::now_v7();
    let o_dewi = Uuid::now_v7();
    let o_rizky = Uuid::now_v7();
    let o_siti = Uuid::now_v7();
    insert_owner(
        &mut tx,
        o_ratna,
        "Ratna Sari",
        "+6281234567801",
        Some("Jl. Kaliurang KM 5, Sleman, Yogyakarta"),
        None,
    )
    .await?;
    insert_owner(
        &mut tx,
        o_agus,
        "Agus Wibowo",
        "+6281234567802",
        Some("Jl. Affandi No. 12, Depok, Sleman"),
        Some("Langganan sejak 2023"),
    )
    .await?;
    insert_owner(
        &mut tx,
        o_dewi,
        "Dewi Anggraini",
        "+6281234567803",
        Some("Jl. Malioboro No. 88, Yogyakarta"),
        None,
    )
    .await?;
    insert_owner(
        &mut tx,
        o_rizky,
        "Rizky Ramadhan",
        "+6281234567804",
        Some("Jl. Parangtritis KM 3, Bantul"),
        None,
    )
    .await?;
    insert_owner(
        &mut tx,
        o_siti,
        "Siti Rahayu",
        "+6281234567805",
        Some("Jl. Godean KM 7, Sleman"),
        None,
    )
    .await?;

    // ---- patients ----
    let p_mochi = Uuid::now_v7();
    let p_oyen = Uuid::now_v7();
    let p_bruno = Uuid::now_v7();
    let p_milo = Uuid::now_v7();
    let p_snowy = Uuid::now_v7();
    let p_cici = Uuid::now_v7();
    let p_lolly = Uuid::now_v7();
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_mochi,
            owner_id: o_ratna,
            name: "Mochi",
            species: Species::Cat,
            breed: Some("Domestic Short Hair"),
            sex: Sex::Female,
            sterilized: true,
            birth_date: NaiveDate::from_ymd_opt(2022, 3, 15),
            color_markings: Some("Abu-abu tabby"),
            allergies: None,
            alert_notes: None,
        },
    )
    .await?;
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_oyen,
            owner_id: o_agus,
            name: "Oyen",
            species: Species::Cat,
            breed: Some("Domestic Short Hair"),
            sex: Sex::Male,
            sterilized: false,
            birth_date: NaiveDate::from_ymd_opt(2021, 8, 1),
            color_markings: Some("Oranye"),
            allergies: None,
            alert_notes: Some("Suka menggigit saat kakinya dipegang"),
        },
    )
    .await?;
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_bruno,
            owner_id: o_dewi,
            name: "Bruno",
            species: Species::Dog,
            breed: Some("Golden Retriever"),
            sex: Sex::Male,
            sterilized: false,
            birth_date: NaiveDate::from_ymd_opt(2020, 7, 1),
            color_markings: Some("Cokelat keemasan"),
            allergies: None,
            alert_notes: None,
        },
    )
    .await?;
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_milo,
            owner_id: o_rizky,
            name: "Milo",
            species: Species::Dog,
            breed: Some("Pomeranian"),
            sex: Sex::Male,
            sterilized: true,
            birth_date: NaiveDate::from_ymd_opt(2023, 1, 20),
            color_markings: Some("Putih krem"),
            allergies: Some("Ayam"),
            alert_notes: None,
        },
    )
    .await?;
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_snowy,
            owner_id: o_siti,
            name: "Snowy",
            species: Species::Rabbit,
            breed: Some("Netherland Dwarf"),
            sex: Sex::Female,
            sterilized: false,
            birth_date: None,
            color_markings: Some("Putih"),
            allergies: None,
            alert_notes: None,
        },
    )
    .await?;
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_cici,
            owner_id: o_ratna,
            name: "Cici",
            species: Species::Rabbit,
            breed: Some("Anggora"),
            sex: Sex::Female,
            sterilized: false,
            birth_date: None,
            color_markings: Some("Putih-abu"),
            allergies: None,
            alert_notes: None,
        },
    )
    .await?;
    insert_patient(
        &mut tx,
        PatientSeed {
            id: p_lolly,
            owner_id: o_agus,
            name: "Lolly",
            species: Species::Bird,
            breed: Some("Lovebird"),
            sex: Sex::Unknown,
            sterilized: false,
            birth_date: None,
            color_markings: Some("Hijau-kuning"),
            allergies: None,
            alert_notes: None,
        },
    )
    .await?;

    // ---- visits ----
    let v_mochi = Uuid::now_v7();
    let v_bruno_vax = Uuid::now_v7();
    let v_bruno_leg = Uuid::now_v7();
    let v_milo = Uuid::now_v7();
    let v_oyen = Uuid::now_v7();
    insert_visit(
        &mut tx,
        VisitSeed {
            id: v_mochi,
            patient_id: p_mochi,
            vet_id: u_citra,
            days_ago: 2,
            complaint: "Tidak mau makan 2 hari, muntah 1 kali",
            temperature_c: Some(39.4),
            weight_kg: Some(3.6),
            exam_notes: Some("Dehidrasi ringan, abdomen tegang saat palpasi"),
            diagnosis: Some("Suspek gastritis"),
            treatment: Some("Injeksi antiemetik, infus subkutan"),
            prescription: Some("Amoxicillin sirup 2x0.5 ml selama 5 hari"),
            follow_up_date: Some(today + Duration::days(3)),
        },
    )
    .await?;
    insert_visit(
        &mut tx,
        VisitSeed {
            id: v_bruno_vax,
            patient_id: p_bruno,
            vet_id: u_bagus,
            days_ago: 60,
            complaint: "Vaksinasi tahunan, kondisi sehat",
            temperature_c: Some(38.6),
            weight_kg: Some(28.4),
            exam_notes: Some("Kondisi umum baik"),
            diagnosis: None,
            treatment: Some("Vaksinasi DHPP"),
            prescription: None,
            follow_up_date: None,
        },
    )
    .await?;
    insert_visit(
        &mut tx,
        VisitSeed {
            id: v_bruno_leg,
            patient_id: p_bruno,
            vet_id: u_bagus,
            days_ago: 7,
            complaint: "Pincang kaki depan kanan setelah bermain",
            temperature_c: Some(38.8),
            weight_kg: Some(29.1),
            exam_notes: Some("Nyeri saat fleksi karpal, tidak ada krepitasi"),
            diagnosis: Some("Sprain ringan"),
            treatment: Some("Meloxicam 3 hari, istirahat dari aktivitas berat"),
            prescription: Some("Meloxicam 0.1 mg/kg 1x sehari selama 3 hari"),
            follow_up_date: Some(today + Duration::days(7)),
        },
    )
    .await?;
    insert_visit(
        &mut tx,
        VisitSeed {
            id: v_milo,
            patient_id: p_milo,
            vet_id: u_citra,
            days_ago: 30,
            complaint: "Gatal-gatal, sering menggaruk telinga dan perut",
            temperature_c: Some(38.5),
            weight_kg: Some(3.1),
            exam_notes: Some("Eritema di abdomen, kulit kering"),
            diagnosis: Some("Dermatitis alergi (suspek alergi pakan)"),
            treatment: Some("Antihistamin, ganti pakan hipoalergenik"),
            prescription: Some("Cetirizine 2.5 mg 1x sehari selama 7 hari"),
            follow_up_date: None,
        },
    )
    .await?;
    insert_visit(
        &mut tx,
        VisitSeed {
            id: v_oyen,
            patient_id: p_oyen,
            vet_id: u_bagus,
            days_ago: 100,
            complaint: "Luka gigitan di telinga kiri setelah berkelahi",
            temperature_c: Some(39.0),
            weight_kg: Some(4.2),
            exam_notes: Some("Laserasi 1 cm di pinna sinistra"),
            diagnosis: Some("Vulnus laceratum"),
            treatment: Some("Pembersihan luka, salep antibiotik"),
            prescription: None,
            follow_up_date: None,
        },
    )
    .await?;

    // ---- vaccinations (windows chosen to exercise dashboard + reminders) ----
    insert_vaccination(
        &mut tx,
        Uuid::now_v7(),
        p_mochi,
        None,
        "Tricat (F3)",
        today - Duration::days(355),
        Some("TC-2412-A"),
        Some(today + Duration::days(10)),
    )
    .await?;
    insert_vaccination(
        &mut tx,
        Uuid::now_v7(),
        p_bruno,
        None,
        "Rabies (Rabisin)",
        today - Duration::days(363),
        Some("RB-2501-11"),
        Some(today + Duration::days(2)),
    )
    .await?;
    insert_vaccination(
        &mut tx,
        Uuid::now_v7(),
        p_bruno,
        Some(v_bruno_vax),
        "DHPP (Vanguard Plus 5)",
        today - Duration::days(60),
        Some("VG-2504-03"),
        Some(today + Duration::days(305)),
    )
    .await?;
    insert_vaccination(
        &mut tx,
        Uuid::now_v7(),
        p_oyen,
        None,
        "Tricat (F3)",
        today - Duration::days(400),
        Some("TC-2406-B"),
        Some(today - Duration::days(35)),
    )
    .await?;
    insert_vaccination(
        &mut tx,
        Uuid::now_v7(),
        p_milo,
        None,
        "Rabies (Rabisin)",
        today - Duration::days(200),
        Some("RB-2412-07"),
        Some(today + Duration::days(165)),
    )
    .await?;

    // ---- appointments ----
    insert_appointment(
        &mut tx,
        Uuid::now_v7(),
        p_milo,
        now + Duration::hours(2),
        "Kontrol dermatitis",
        AppointmentStatus::Scheduled,
        None,
    )
    .await?;
    insert_appointment(
        &mut tx,
        Uuid::now_v7(),
        p_mochi,
        now + Duration::hours(4),
        "Kontrol pasca rawat gastritis",
        AppointmentStatus::Scheduled,
        None,
    )
    .await?;
    insert_appointment(
        &mut tx,
        Uuid::now_v7(),
        p_bruno,
        now - Duration::days(1),
        "Vaksinasi rabies",
        AppointmentStatus::Done,
        None,
    )
    .await?;
    insert_appointment(
        &mut tx,
        Uuid::now_v7(),
        p_oyen,
        now + Duration::days(3),
        "Vaksinasi ulang Tricat",
        AppointmentStatus::Scheduled,
        Some("Ingatkan owner membawa kandang"),
    )
    .await?;

    // ---- inventory: items + movement ledger ----
    let i_amox = Uuid::now_v7();
    let i_melox = Uuid::now_v7();
    let i_rabisin = Uuid::now_v7();
    let i_felocell = Uuid::now_v7();
    let i_spuit = Uuid::now_v7();
    let i_nacl = Uuid::now_v7();
    insert_item(
        &mut tx,
        i_amox,
        "Amoxicillin Sirup Kering 60 ml",
        InventoryCategory::Drug,
        "botol",
        5.0,
        Some(today + Duration::days(45)),
    )
    .await?;
    insert_item(
        &mut tx,
        i_melox,
        "Meloxicam 1.5 mg/ml 10 ml",
        InventoryCategory::Drug,
        "botol",
        3.0,
        Some(today + Duration::days(20)),
    )
    .await?;
    insert_item(
        &mut tx,
        i_rabisin,
        "Rabisin (vaksin rabies)",
        InventoryCategory::Vaccine,
        "vial",
        10.0,
        Some(today + Duration::days(180)),
    )
    .await?;
    insert_item(
        &mut tx,
        i_felocell,
        "Felocell 3 (vaksin kucing)",
        InventoryCategory::Vaccine,
        "vial",
        5.0,
        Some(today + Duration::days(240)),
    )
    .await?;
    insert_item(
        &mut tx,
        i_spuit,
        "Spuit 3 ml",
        InventoryCategory::Supply,
        "pcs",
        50.0,
        None,
    )
    .await?;
    insert_item(
        &mut tx,
        i_nacl,
        "Infus NaCl 0.9% 500 ml",
        InventoryCategory::Drug,
        "botol",
        10.0,
        Some(today + Duration::days(300)),
    )
    .await?;

    // stock: amox 12-1-4=7, melox 6-2=4, rabisin 10-6=4 (below min 10!),
    // felocell 15-3=12, spuit 100-15-2=83, nacl 24-6=18
    insert_movement(
        &mut tx,
        i_amox,
        MovementType::In,
        12.0,
        90,
        Some("Pembelian PT Sanbe Farma"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_amox,
        MovementType::Out,
        1.0,
        2,
        Some("Terpakai kunjungan Mochi"),
        Some(v_mochi),
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_amox,
        MovementType::Out,
        4.0,
        30,
        Some("Pemakaian rutin"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_melox,
        MovementType::In,
        6.0,
        120,
        Some("Pembelian PT Medion"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_melox,
        MovementType::Out,
        2.0,
        7,
        Some("Terapi sprain Bruno"),
        Some(v_bruno_leg),
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_rabisin,
        MovementType::In,
        10.0,
        150,
        Some("Pembelian distributor vaksin"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_rabisin,
        MovementType::Out,
        6.0,
        20,
        Some("Program vaksinasi rabies"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_felocell,
        MovementType::In,
        15.0,
        150,
        Some("Pembelian distributor vaksin"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_felocell,
        MovementType::Out,
        3.0,
        40,
        Some("Vaksinasi kucing"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_spuit,
        MovementType::In,
        100.0,
        60,
        Some("Pembelian alat habis pakai"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_spuit,
        MovementType::Out,
        15.0,
        14,
        Some("Pemakaian rutin"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_spuit,
        MovementType::Adjustment,
        -2.0,
        5,
        Some("Stok opname: 2 pcs rusak"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_nacl,
        MovementType::In,
        24.0,
        90,
        Some("Pembelian PT Widatra"),
        None,
        now,
    )
    .await?;
    insert_movement(
        &mut tx,
        i_nacl,
        MovementType::Out,
        6.0,
        10,
        Some("Pemakaian rutin"),
        None,
        now,
    )
    .await?;

    tx.commit().await?;

    tracing::info!(
        users = 2,
        owners = 5,
        patients = 7,
        visits = 5,
        vaccinations = 5,
        appointments = 4,
        inventory_items = 6,
        "seed complete"
    );
    tracing::info!(
        "login: citra@citrapetcare.id / bagus@citrapetcare.id, password: {seed_password} (set SEED_PASSWORD to override; change in production!)"
    );
    Ok(())
}

async fn insert_user(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    email: &str,
    password_hash: &str,
    role: UserRole,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO users (id, name, email, password_hash, role) VALUES ($1, $2, $3, $4, $5)",
        id,
        name,
        email,
        password_hash,
        role as UserRole
    )
    .execute(conn)
    .await?;
    Ok(())
}

async fn insert_owner(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    phone: &str,
    address: Option<&str>,
    notes: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO owners (id, name, phone, address, notes) VALUES ($1, $2, $3, $4, $5)",
        id,
        name,
        phone,
        address,
        notes
    )
    .execute(conn)
    .await?;
    Ok(())
}

struct PatientSeed {
    id: Uuid,
    owner_id: Uuid,
    name: &'static str,
    species: Species,
    breed: Option<&'static str>,
    sex: Sex,
    sterilized: bool,
    birth_date: Option<NaiveDate>,
    color_markings: Option<&'static str>,
    allergies: Option<&'static str>,
    alert_notes: Option<&'static str>,
}

async fn insert_patient(conn: &mut PgConnection, p: PatientSeed) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO patients (id, owner_id, name, species, breed, sex, sterilized, birth_date,
                              color_markings, allergies, alert_notes, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        p.id,
        p.owner_id,
        p.name,
        p.species as Species,
        p.breed,
        p.sex as Sex,
        p.sterilized,
        p.birth_date,
        p.color_markings,
        p.allergies,
        p.alert_notes,
        PatientStatus::Active as PatientStatus
    )
    .execute(conn)
    .await?;
    Ok(())
}

struct VisitSeed {
    id: Uuid,
    patient_id: Uuid,
    vet_id: Uuid,
    days_ago: i64,
    complaint: &'static str,
    temperature_c: Option<f64>,
    weight_kg: Option<f64>,
    exam_notes: Option<&'static str>,
    diagnosis: Option<&'static str>,
    treatment: Option<&'static str>,
    prescription: Option<&'static str>,
    follow_up_date: Option<NaiveDate>,
}

async fn insert_visit(conn: &mut PgConnection, v: VisitSeed) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO visits (id, patient_id, vet_id, visit_date, complaint, temperature_c,
                            weight_kg, exam_notes, diagnosis, treatment, prescription, follow_up_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        v.id,
        v.patient_id,
        v.vet_id,
        Utc::now() - Duration::days(v.days_ago),
        v.complaint,
        v.temperature_c,
        v.weight_kg,
        v.exam_notes,
        v.diagnosis,
        v.treatment,
        v.prescription,
        v.follow_up_date
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // flat args keep the seed rows readable
async fn insert_vaccination(
    conn: &mut PgConnection,
    id: Uuid,
    patient_id: Uuid,
    visit_id: Option<Uuid>,
    vaccine_name: &str,
    date_given: NaiveDate,
    batch_no: Option<&str>,
    next_due_date: Option<NaiveDate>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO vaccinations (id, patient_id, visit_id, vaccine_name, date_given, batch_no, next_due_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        id,
        patient_id,
        visit_id,
        vaccine_name,
        date_given,
        batch_no,
        next_due_date
    )
    .execute(conn)
    .await?;
    Ok(())
}

async fn insert_appointment(
    conn: &mut PgConnection,
    id: Uuid,
    patient_id: Uuid,
    scheduled_at: chrono::DateTime<Utc>,
    reason: &str,
    status: AppointmentStatus,
    notes: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO appointments (id, patient_id, scheduled_at, reason, status, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        id,
        patient_id,
        scheduled_at,
        reason,
        status as AppointmentStatus,
        notes
    )
    .execute(conn)
    .await?;
    Ok(())
}

async fn insert_item(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    category: InventoryCategory,
    unit: &str,
    min_stock: f64,
    expiry_date: Option<NaiveDate>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO inventory_items (id, name, category, unit, min_stock, expiry_date)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        id,
        name,
        category as InventoryCategory,
        unit,
        min_stock,
        expiry_date
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // flat args keep the seed rows readable
async fn insert_movement(
    conn: &mut PgConnection,
    item_id: Uuid,
    movement_type: MovementType,
    qty: f64,
    days_ago: i64,
    reason: Option<&str>,
    visit_id: Option<Uuid>,
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO stock_movements (id, item_id, type, qty, reason, visit_id, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        Uuid::now_v7(),
        item_id,
        movement_type as MovementType,
        qty,
        reason,
        visit_id,
        now - Duration::days(days_ago)
    )
    .execute(conn)
    .await?;
    Ok(())
}
