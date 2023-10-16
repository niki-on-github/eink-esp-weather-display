use std::collections::HashMap;

use chrono::prelude::*;
use chrono::Duration;
//use chrono::{DateTime, FixedOffset};
use reqwest::blocking::Client;
use serde_json::Value;

use crate::EnvData;

const OBSERVATION_DATA_URL: &'static str = "https://api.weather.gov/stations/{station}/observations?limit=1";
const DAILY_FORECAST_URL: &'static str = "https://api.weather.gov/gridpoints/{office}/{gridpoint}/forecast";
const HOURLY_FORECAST_URL: &'static str = "https://api.weather.gov/gridpoints/{office}/{gridpoint}/forecast/hourly";


pub type FullForecast = Vec<(DateTime<FixedOffset>, i32, u64)>;
pub type FilteredForecast = Vec<(DateTime<FixedOffset>, i32, u64)>;

// number of data points to filter by (average rain probabilities etc)
const CHUNK_SIZE: usize = 3;

#[derive(Clone)]
/// Forecast data for the next 5 days
pub struct Forecast5Day {
    pub full_forecast: FullForecast,
}

impl Forecast5Day {
    pub fn new(hourly_forecast: &[ForecastPeriod]) -> Forecast5Day {
        let mut full_forecast: FullForecast = Vec::new();

        let start_dt = hourly_forecast[0].start_time;
        let five_days = Duration::days(5);
        for s in hourly_forecast {
            if s.start_time - start_dt > five_days {
                break;
            }
            full_forecast.push((s.start_time, s.temp_f, s.rain_prob));
        }

        // make sure forecast length is divisible by 3 for filtering later
        let rem = full_forecast.len() % CHUNK_SIZE;
        for _ in 0..rem { full_forecast.pop(); }

        Forecast5Day {
            full_forecast,
        }
    }

    /// Returns weekly and daily temperatures as (min, max, dailyminmax)
    /// Keys are simply the day of the month which will not overlap as long as the forecast is 5
    /// days (i.e. less than a full month)
    pub fn daily_minmax_temps(&self) -> HashMap<u32, (i32, i32)> {
        let mut day_min: i32 = 200;
        let mut day_max: i32 = -100;
        let mut daily_minmax: HashMap<u32, (i32, i32)> = HashMap::new();

        let mut current_day: u32 = self.full_forecast[0].0.day();
        for (d, t, _) in &self.full_forecast {
            let day = d.day();
            let t = *t;

            if day != current_day {
                daily_minmax.insert(current_day, (day_min, day_max));
                day_min = 200;
                day_max = -100;
                current_day = day;
            }
            if t < day_min && d.hour() > 12 {
                day_min = t;
            }
            if t > day_max {
                day_max = t;
            }
        }
        // below is commented to ignore update for the last day
        // if day != current_day {
        //     daily_minmax.insert(current_day, (day_min, day_max));
        // }
 
        daily_minmax
    }

    /// Returns min and max temps for the week
    pub fn week_minmax_temps(&self) -> (i32, i32) {
        let min_temp = self.full_forecast.iter()
            .min_by_key(|(_, t, _)| t).expect("no values in forecast").1;
        let max_temp = self.full_forecast.iter()
            .max_by_key(|(_, t, _)| t).expect("no values in forecast").1;
        (min_temp, max_temp)
    }

    /// Returns the temperature and rain data filtered to be smoother for drawing on a graph.
    pub fn filtered_forecast(&self) -> FilteredForecast {
        let mut forecast = Vec::new();

        for c in self.full_forecast.chunks(CHUNK_SIZE) {
            let len = c.len();
            if len != CHUNK_SIZE {
                panic!("forecast chunk size was not divisible by {CHUNK_SIZE}");
            }
            let start_time = c[1].0;
            let temp = c[1].1;
            let avg_precip = c.iter().map(|t| t.2).sum::<u64>()/len as u64;

            forecast.push((start_time, temp, avg_precip));
        }

        forecast
    }
}

#[derive(Debug, Clone)]
pub struct CurrentWeather {
    pub description: String,
    pub temp_f: i32,
    // it was coming back as None sometimes and I'm not using it anyway
    // pub wind_speed: u32,
    pub rain_in: u32,
}

#[derive(Debug, Clone)]
pub struct ForecastPeriod {
    pub period_name: Option<String>,
    pub start_time: DateTime<FixedOffset>,
    pub end_time: DateTime<FixedOffset>,
    pub temp_f: i32,
    /// percentage out of 100
    pub rain_prob: u64,
    pub wind_speed: u64,
    pub short_desc: String,
    pub long_desc: Option<String>,
}

// deprecated in favor of Forecast5Day::filtered_forecast
// /// returns two vecs of (start_hour, temperature, rain_probability)
// pub(crate) fn gather_5day_forecast(hourly_forecast: &[ForecastPeriod]) -> (FullForecast, AvgForecast) {
//     let mut avg_out = Vec::new();
//     let mut full_out = Vec::new();
// 
//     for s in hourly_forecast {
//         full_out.push((s.start_time, s.temp_f, s.rain_prob));
//     }
// 
//     for s in hourly_forecast.chunks(4) {
//         let len = s.len();
//         let start_time = s[0].start_time;
//         let avg_temp = s.iter().map(|h| h.temp_f).sum::<i32>()/len as i32;
//         let avg_precip = s.iter().map(|h| h.rain_prob).sum::<u64>()/len as u64;
// 
//         avg_out.push((start_time, avg_temp, avg_precip));
//     }
// 
//     let start = avg_out[0].0;
//     let dt = chrono::Duration::days(5);
//     let avg_output = avg_out.into_iter().filter(|(d,_,_)| *d - start < dt).collect();
//     let full_output = full_out.into_iter().filter(|(d,_,_)| *d - start < dt).collect();
// 
//     return (full_output, avg_output);
// }


pub fn create_weather_client(env_data: &EnvData) -> Client {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/geo+json".parse().unwrap());
    reqwest::blocking::Client::builder()
        .user_agent(&env_data.user_agent)
        .default_headers(headers)
        .build().expect("couldn't create requests client")
}

pub fn get_current_weather(env_data: &EnvData, client: &Client) -> String {
    let url = OBSERVATION_DATA_URL.replace("{station}", &env_data.weather_station);
    let json_str = client.get(url)
        .query(&[("limit", "1")])
        .send()
        .expect("failed to make current observation request")
        .text()
        .expect("failed to get text from current observation request");

    return json_str
}

pub fn parse_current_weather(json_str: &str) -> CurrentWeather {
    let data: Value = serde_json::from_str(json_str)
        .expect("failed to parse current observation json");

    //println!("current weather");
    //println!("{:#?}", data);

    let data = &data["features"][0]["properties"];

    let temp_f = data["temperature"]["value"].as_f64().unwrap()*1.8 + 32.0;
    let temp_f = temp_f as i32;
    
    // convert km to m
    // let wind_speed = data["windSpeed"]["value"].as_f64().unwrap()*0.621;
    // let wind_speed = wind_speed as u32;

    let rain_in = data["precipitationLastHour"]["value"].as_f64().unwrap_or(0.0)*0.039;
    let rain_in = rain_in as u32;

    let description = data["textDescription"].as_str().unwrap().into();

    CurrentWeather {
        description,
        temp_f,
        //wind_speed,
        rain_in,
    }

}

pub fn get_daily_forecast(env_data: &EnvData, client: &Client) -> String {
    let url = DAILY_FORECAST_URL.replace("{office}", &env_data.weather_office)
        .replace("{gridpoint}", &env_data.weather_gridpoint);
    let json_str = client.get(url).send()
        .expect("failed to make daily forecast request")
        .text()
        .expect("failed to get text from daily forecast request");

    return json_str;
}

pub fn parse_daily_forecast(json_str: &str) -> Vec<ForecastPeriod> {
    let data: Value = serde_json::from_str(&json_str)
        .expect("failed to parse hourly forecast json");

    println!("debug data:\n{:#?}", data);

    let periods = data["properties"]["periods"].as_array()
        .expect("properties.periods was not an array");

    let mut output = Vec::new();
    for period in periods {
        let period_name = period["name"].as_str().unwrap().parse().ok();
        let start_time = DateTime::parse_from_rfc3339(&period["startTime"].as_str().unwrap())
            .expect("failed to parse start time datetime");
        let end_time = DateTime::parse_from_rfc3339(&period["endTime"].as_str().unwrap())
            .expect("failed to parse end time datetime");
        let temp_f = period["temperature"].as_i64().unwrap() as i32;

        let rain_prob = period["probabilityOfPrecipitation"]["value"].as_u64().unwrap_or(0);

        let wind_speed = period["windSpeed"].as_str().unwrap().split(' ')
            .filter_map(|s| s.parse::<u64>().ok())
            .max()
            .unwrap_or(0);

        let short_desc = period["shortForecast"].as_str().unwrap().to_string();
        let long_desc_val = period["detailedForecast"].as_str().unwrap().to_string();
        let long_desc = if long_desc_val.len() == 0 { None } else { Some(long_desc_val) };

        let forecast = ForecastPeriod {
            period_name,
            start_time,
            end_time,
            temp_f,
            rain_prob,
            wind_speed,
            short_desc,
            long_desc,
        };
        output.push(forecast);
    }

    output
}

pub fn get_hourly_forecast(env_data: &EnvData, client: &Client) -> String {
    let url = HOURLY_FORECAST_URL.replace("{office}", &env_data.weather_office)
        .replace("{gridpoint}", &env_data.weather_gridpoint);
    let json_str = client.get(url).send()
        .expect("failed to make hourly forecast request")
        .text()
        .expect("failed to get text from hourly forecast request");

    return json_str;
}

pub fn parse_hourly_forecast(json_str: &str) -> Vec<ForecastPeriod> {
    let data: Value = serde_json::from_str(&json_str)
        .expect("failed to parse hourly forecast json");

    let periods = data["properties"]["periods"].as_array()
        .expect("properties.periods was not an array");

    let mut output = Vec::new();
    for period in periods {
        let start_time = DateTime::parse_from_rfc3339(&period["startTime"].as_str().unwrap())
            .expect("failed to parse start time datetime");
        let end_time = DateTime::parse_from_rfc3339(&period["endTime"].as_str().unwrap())
            .expect("failed to parse end time datetime");
        let period_name = Some(start_time.format("%a %k%P").to_string());
        let temp_f = period["temperature"].as_i64().unwrap() as i32;

        let rain_prob_value = &period["probabilityOfPrecipitation"]["value"];
        let rain_prob: u64;
        if rain_prob_value.is_null() {
            rain_prob = 0;
        }
        else {
            rain_prob = rain_prob_value.as_u64().unwrap();
        }

        let wind_speed = period["windSpeed"].as_str().unwrap().split(' ')
            .filter_map(|s| s.parse::<u64>().ok())
            .max()
            .unwrap_or(0);

        let short_desc = period["shortForecast"].as_str().unwrap().to_string();
        let long_desc = None;

        let forecast = ForecastPeriod {
            period_name,
            start_time,
            end_time,
            temp_f,
            rain_prob,
            wind_speed,
            short_desc,
            long_desc,
        };
        output.push(forecast);
    }

    output
}
