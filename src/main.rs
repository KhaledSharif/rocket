#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;

use rocket::response::status;
use rocket::State;

#[macro_use] extern crate serde_derive;

extern crate mongodb;

extern crate rocket_contrib;
use rocket_contrib::json::Json;

extern crate rocket_cors;
use rocket_cors::{Cors, AllowedOrigins, AllowedHeaders, AllowedMethods};

use std::str::FromStr;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use std::time::SystemTime;

use mongodb::{bson, doc, Client, ThreadedClient};
use mongodb::coll::Collection;
use mongodb::db::ThreadedDatabase;
use mongodb::Document;

fn get_time() -> u64
{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[derive(Serialize, Deserialize)]
struct Message
{
    key   : String,
    value : String,
    time  : u64,
}

impl Message
{
    fn to_document(&self) -> Document
    {
        doc!
        {
            "time" : self.time,
            "key"  : self.key.clone(),
            "value": self.value.clone(),
        }
    }
}

trait DocumentToMessage
{
    fn to_message(&self) -> Message;
    fn insert_into(self, &Collection);
}

impl DocumentToMessage for Document
{
    fn to_message(&self) -> Message
    {
        Message
        {
            key   : self.get_str("key").ok().unwrap().to_string(),
            value : self.get_str("value").ok().unwrap().to_string(),
            time  : self.get_i64("time").ok().unwrap() as u64,
        }
    }
    fn insert_into(self, mc: &Collection)
    {
        mc.insert_one(self, None).expect("failed to insert document");
    }
}

fn get_documents(get_request : &GetRequest, mongo_collection: &Collection) -> Vec<Document>
{
    mongo_collection
        .find(Some(get_request.to_query()), None)
        .expect("failed to execute find")
        .take_while(|x| x.is_ok())
        .map(|x| x.unwrap())
        .collect::<Vec<Document>>()
}

struct GetRequest
{
    key     : String,
    time_gt : Option<u64>,
    time_lt : Option<u64>,
}

impl GetRequest
{
    fn to_query(&self) -> Document
    {
        let mut document = doc!{
            "key": self.key.clone(),
        };
        let lt = self.time_lt.is_some();
        let gt = self.time_gt.is_some();
        let mut time_document = doc!{};
        if gt {
            time_document.insert("$gt", self.time_gt.unwrap());
        }
        if lt {
            time_document.insert("$lt", self.time_lt.unwrap());
        }
        if gt || lt {
            document.insert("time".to_string(), time_document);
        }
        document
    }
}

struct PostRequest
{
    time  : u64,
    key   : String,
    value : String,
}

impl PostRequest
{
    fn to_message(&self) -> Message
    {
        Message
        {
            key   : self.key.clone(),
            value : self.value.clone(),
            time  : self.time,
        }
    }
}

struct OptionsArray
{
    keys   : Vec<String>,
    values : Vec<String>,
}

impl OptionsArray
{
    fn get(&self, key: &str) -> Option <&String>
    {
        for i in 0..self.keys.len()
        {
            if key == self.keys.get(i).unwrap()
            {
                return self.values.get(i);
            }
        }
        return None;
    }
    fn contains_key(&self, key: &str) -> bool
    {
        self.keys.contains(&key.to_string())
    }
    fn insert(&mut self, key: &str, value: &str)
    {
        self.keys.push(key.to_string());
        self.values.push(value.to_string());
    }
    fn new() -> Self
    {
        OptionsArray
        {
            keys:   Vec::new(),
            values: Vec::new(),
        }
    }
}

fn create_options_map(options: PathBuf) -> Option <OptionsArray>
{
    let options_string_array : Vec<&str> =
        options
            .iter()
            .map(|x| x.to_str().unwrap())
            .collect();
    if options_string_array.len() % 2 != 0
    {
        return None;
    }
    let mut i = 0;
    let mut options_array : OptionsArray = OptionsArray::new();
    while i < (options_string_array.len() - 1)
    {
        options_array.insert(
            options_string_array[i],
            options_string_array[i + 1],
        );
        i += 2;
    }
    Some(options_array)
}

#[post("/message/<options..>")]
fn post(state: State<Collection>, options: PathBuf) -> Result <status::Created <String>, status::BadRequest <String>>
{
    let options_array : OptionsArray;
    match create_options_map(options)
    {
        Some(x) => {
            options_array = x;
        },
        None => {
            return Err(status::BadRequest(None));
        }
    }
    let post_request = PostRequest
    {
        time:  get_time(),
        key:   options_array.get("key").unwrap().clone(),
        value: options_array.get("value").unwrap().clone(),
    };
    post_request.to_message().to_document().insert_into(
        &state.inner()
    );
    Ok(status::Created("".to_string(), None))
}

#[get("/message/<options..>")]
fn get(state: State<Collection>, options: PathBuf) -> Result <Json <Vec <Message>>, status::BadRequest <String>>
{
    let options_array : OptionsArray;
    match create_options_map(options)
    {
        Some(x) =>
        {
            options_array = x;
        },
        None =>
        {
            return Err(status::BadRequest(None));
        }
    }
    fn insert_time_option(json_key: &str, oa: &OptionsArray) -> Option <u64>
    {
        if oa.contains_key(json_key)
        {
            return Some(
                oa.get(json_key).unwrap().clone().parse::<u64>().unwrap()
            );
        }
        None
    }
    let get_request = GetRequest
    {
        key:     options_array.get("key").unwrap().clone(),
        time_lt: insert_time_option("time_lt", &options_array),
        time_gt: insert_time_option("time_gt", &options_array),
    };
    let documents : Vec<Document> = get_documents(&get_request, &state.inner());
    let messages  : Vec<Message>  = documents.iter().map(|x| x.to_message()).collect();
    Ok(Json(messages))
}

fn rocket(mc: Collection, cors: Cors) -> rocket::Rocket
{
    rocket::ignite()
        .mount("/", routes![get, post])
        .attach(cors)
        .manage(mc)
}

fn main()
{
    // =====================================================
    // This Rocket instance uses a State
    // see: https://github.com/SergioBenitez/Rocket/issues/53#issuecomment-277149045
    // =====================================================
    let mongo_collection = Client::connect("mongo", 27017)
        .expect("failed to initialize mongo client")
        .db("iot")
        .collection("key_value");

    // =====================================================
    // This Rocket instance uses rocket-cors for CORS options
    // see: https://lawliet89.github.io/rocket_cors/rocket_cors/index.html#fairing
    // =====================================================
    let allowed_origins: AllowedOrigins = AllowedOrigins::all();
    let allowed_methods: AllowedMethods = ["Get", "Post", "Delete"]
        .iter()
        .map(|s| FromStr::from_str(s).unwrap())
        .collect();
    let allowed_headers: AllowedHeaders = AllowedHeaders::all();

    let options = rocket_cors::Cors
    {
        allowed_origins,
        allowed_methods,
        allowed_headers,
        allow_credentials: true,
        ..Default::default()
    };

    rocket(mongo_collection, options).launch();
}
