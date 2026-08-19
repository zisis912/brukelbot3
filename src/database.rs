use regex::Regex;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let sql_pool = SqlitePool::connect("sqlite://data/database.db?mode=rwc")
            .await
            .unwrap();

        sqlx::migrate!("./migrations").run(&sql_pool).await?;

        Ok(Self { pool: sql_pool })
    }

    pub async fn nigga_increment(&self, user_id: u64, increment: i64) -> Result<(), sqlx::Error> {
        println!("{},{}", user_id, increment);
        sqlx::query!(
            "INSERT INTO nigga_leaderboard VALUES(?,?)
                ON CONFLICT(user_id) DO UPDATE SET nigga_count = nigga_count + ?;",
            user_id as i64,
            increment,
            increment
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn nigga_balance(&self, user_id: u64) -> Result<i64, sqlx::Error> {
        let nigbal: i64 = sqlx::query!(
            r#"SELECT nigga_count AS "nigbal!" FROM nigga_leaderboard WHERE user_id = ?;"#,
            user_id as i64
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.nigbal)
        .unwrap_or(0);

        Ok(nigbal)
    }

    pub async fn get_rankings(&self) -> Result<Vec<Ranking>, sqlx::Error> {
        sqlx::query_as!(
            Ranking,
            r#"SELECT user_id AS "id!", nigga_count AS "count!" FROM nigga_leaderboard;"#
        )
        .fetch_all(&self.pool)
        .await
    }
}

pub struct Ranking {
    pub id: i64,
    pub count: i64,
}
