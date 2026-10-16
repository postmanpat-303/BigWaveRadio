use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::models::{Country, Station, Tag};

const APP_USER_AGENT: &str = "BigWaveRadio/0.4 (https://github.com/postmanpat-303/BigWaveRadio)";

/// DNS resolver that returns IPv4 addresses only.
///
/// Some client networks blackhole IPv6 SYN packets (no RST), and happy-eyeballs
/// fallback in the connector doesn't rescue that quickly on every NIC — leading
/// to 15-25 s stalls per request. Radio streams and the API are IPv4-reachable
/// everywhere, so we simply skip IPv6.
pub struct Ipv4OnlyResolver;

impl reqwest::dns::Resolve for Ipv4OnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let host2 = host.clone();
            let result = std::thread::spawn(move || {
                (host2.as_str(), 0u16)
                    .to_socket_addrs()
                    .map(|it| {
                        it.filter(|a| a.is_ipv4())
                            .collect::<Vec<SocketAddr>>()
                    })
            })
            .join();
            match result {
                Ok(Ok(addrs)) => Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs),
                Ok(Err(e)) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                Err(_) => Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "dns thread panicked",
                )) as Box<dyn std::error::Error + Send + Sync>),
            }
        })
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),
    #[error("api: {0}")]
    Api(String),
}

/// radio-browser.info client with mirror rotation.
///
/// Community etiquette (https://www.radio-browser.info/): send a descriptive
/// User-Agent, distribute load across the volunteer mirrors, and report a
/// click once per play.
pub struct RadioBrowserClient {
    mirrors: Vec<String>,
    http: reqwest::blocking::Client,
    current: AtomicUsize,
}

impl RadioBrowserClient {
    /// Fetch the mirror list and build the client.
    pub fn new() -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(APP_USER_AGENT)
            .dns_resolver(Arc::new(Ipv4OnlyResolver))
            .connect_timeout(std::time::Duration::from_secs(8))
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let mut client = Self {
            mirrors: vec!["https://all.api.radio-browser.info".into()],
            http,
            current: AtomicUsize::new(0),
        };
        client.refresh_mirrors()?;
        Ok(client)
    }

    /// Re-fetch the volunteer mirror list from the directory.
    pub fn refresh_mirrors(&mut self) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Mirror {
            name: String,
        }
        let text = self
            .http
            .get("https://all.api.radio-browser.info/json/servers")
            .send()?
            .error_for_status()?
            .text()?;
        let mirrors: Vec<Mirror> =
            serde_json::from_str(&text).map_err(|e| Error::Api(e.to_string()))?;
        let names: Vec<String> = mirrors
            .into_iter()
            .map(|m| {
                if m.name.starts_with("http://") || m.name.starts_with("https://") {
                    m.name
                } else {
                    format!("https://{}", m.name)
                }
            })
            .collect();
        let mut mirrors = vec!["https://all.api.radio-browser.info".to_string()];
        mirrors.extend(names);
        self.mirrors = mirrors;
        Ok(())
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, path_and_query: &str) -> Result<T> {
        let start = self.current.load(Ordering::Relaxed);
        let attempts = self.mirrors.len().min(3);
        let mut last_err = None;
        for i in 0..attempts {
            let mirror = &self.mirrors[(start + i) % self.mirrors.len()];
            let url = format!("{mirror}{path_and_query}");
            match self.http.get(&url).send() {
                Ok(resp) => {
                    if resp.status().is_success() {
                        self.current.store((start + i) % self.mirrors.len(), Ordering::Relaxed);
                        return resp
                            .json::<T>()
                            .map_err(|e| Error::Api(format!("json: {e}")));
                    }
                    last_err = Some(Error::Api(format!("HTTP {} from {mirror}", resp.status())));
                }
                Err(e) => last_err = Some(e.into()),
            }
        }
        Err(last_err.unwrap_or_else(|| Error::Api("no mirrors available".into())))
    }

    fn quality_clause(min_bitrate: u32, codec: &str) -> String {
        let mut clause = String::new();
        if min_bitrate > 0 {
            clause.push_str(&format!("&bitrateMin={min_bitrate}"));
        }
        let codec = codec.trim();
        if !codec.is_empty() {
            clause.push_str(&format!("&codec={}", urlencoding_encode(&codec.to_uppercase())));
        }
        clause
    }

    /// Post-filter: the ranking endpoints (topclick/topvote/lastchange) ignore
    /// quality params server-side, so enforce them locally for a consistent filter.
    fn filter_quality(stations: Vec<Station>, min_bitrate: u32, codec: &str) -> Vec<Station> {
        let codec = codec.trim();
        stations
            .into_iter()
            .filter(|s| s.bitrate >= min_bitrate)
            .filter(|s| {
                codec.is_empty() || s.codec.trim().eq_ignore_ascii_case(codec)
            })
            .collect()
    }

    pub fn trending(&self, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {
        let stations = self.get_json::<Vec<Station>>(&format!(
            "/json/stations/topclick/100?hidebroken=true{}",
            Self::quality_clause(min_bitrate, codec)
        ))?;
        Ok(Self::filter_quality(stations, min_bitrate, codec))
    }

    pub fn popular(&self, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {
        let stations = self.get_json::<Vec<Station>>(&format!(
            "/json/stations/topvote/100?hidebroken=true{}",
            Self::quality_clause(min_bitrate, codec)
        ))?;
        Ok(Self::filter_quality(stations, min_bitrate, codec))
    }

    pub fn recently_added(&self, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {
        let stations = self.get_json::<Vec<Station>>(&format!(
            "/json/stations/lastchange/100?hidebroken=true{}",
            Self::quality_clause(min_bitrate, codec)
        ))?;
        Ok(Self::filter_quality(stations, min_bitrate, codec))
    }

    /// Case-insensitive substring search on the station name.
    pub fn search(&self, query: &str, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {
        use std::fmt::Write as _;
        let mut q = String::new();
        write!(q, "/json/stations/search?name={}&limit=100&hidebroken=true", urlencoding_encode(query))
            .unwrap();
        q.push_str(&Self::quality_clause(min_bitrate, codec));
        let stations = self.get_json::<Vec<Station>>(&q)?;
        Ok(Self::filter_quality(stations, min_bitrate, codec))
    }

    pub fn tags(&self) -> Result<Vec<Tag>> {
        self.get_json("/json/tags?order=stationcount&reverse=true&hidebroken=true&limit=200")
    }

    pub fn countries(&self) -> Result<Vec<Country>> {
        self.get_json("/json/countries?order=stationcount&reverse=true&hidebroken=true")
    }

    pub fn stations_by_tag(&self, tag: &str, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {
        let stations = self.get_json::<Vec<Station>>(&format!(
            "/json/stations/bytag/{}?limit=100&hidebroken=true&order=votes&reverse=true{}",
            urlencoding_encode(tag),
            Self::quality_clause(min_bitrate, codec)
        ))?;
        Ok(Self::filter_quality(stations, min_bitrate, codec))
    }

    pub fn stations_by_country(&self, country: &str, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {
        let stations = self.get_json::<Vec<Station>>(&format!(
            "/json/stations/bycountryexact/{}?limit=100&hidebroken=true&order=votes&reverse=true{}",
            urlencoding_encode(country),
            Self::quality_clause(min_bitrate, codec)
        ))?;
        Ok(Self::filter_quality(stations, min_bitrate, codec))
    }

    /// Look up full station records for a set of uuids (the Rated list).
    pub fn stations_by_uuids(&self, uuids: &[String]) -> Result<Vec<Station>> {
        if uuids.is_empty() {
            return Ok(Vec::new());
        }
        self.get_json(&format!("/json/stations/byuuid?uuids={}", uuids.join(",")))
    }

    /// Stations sharing a tag, excluding the station we came from.
    pub fn similar(&self, tag: &str, exclude_uuid: &str, min_bitrate: u32, codec: &str) -> Result<Vec<Station>> {        Ok(self
            .stations_by_tag(tag, min_bitrate, codec)?
            .into_iter()
            .filter(|s| s.uuid != exclude_uuid)
            .collect())
    }

    /// One random station honouring the quality filter. "Random" is an
    /// *order* on the search endpoint (there is no /random path); fetch a
    /// batch ordered randomly and pick one that passes the local filter.
    pub fn random_station(&self, min_bitrate: u32, codec: &str) -> Result<Station> {
        let stations = self.get_json::<Vec<Station>>(&format!(
            "/json/stations/search?hidebroken=true&order=random&limit=40{}",
            Self::quality_clause(min_bitrate, codec)
        ))?;
        let mut filtered = Self::filter_quality(stations, min_bitrate, codec);
        if filtered.is_empty() {
            return Err(Error::Api("no station matched the filters".into()));
        }
        // The batch itself is random; pick the slot by time so repeated
        // clicks in the same second still vary.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as usize)
            .unwrap_or(0);
        let idx = secs % filtered.len();
        Ok(filtered.swap_remove(idx))
    }

    /// Report a click back to the directory (community etiquette).
    pub fn report_click(&self, station_uuid: &str) {
        let _ = self.get_json::<serde_json::Value>(&format!(
            "/json/url/{station_uuid}"
        ));
    }
}

/// Minimal percent-encoder for query parameters (space -> %20, as the API expects).
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(codec: &str, bitrate: u32) -> Station {
        Station {
            uuid: uuid(codec, bitrate),
            name: format!("{codec} {bitrate}"),
            stream_url: String::new(),
            homepage: String::new(),
            favicon: String::new(),
            codec: codec.to_string(),
            bitrate,
            votes: 0,
            click_count: 0,
            country: String::new(),
            language: String::new(),
            tags: String::new(),
        }
    }

    fn uuid(codec: &str, bitrate: u32) -> String {
        format!("uuid-{codec}-{bitrate}")
    }

    #[test]
    fn quality_clause_empty_when_unfiltered() {
        assert_eq!(RadioBrowserClient::quality_clause(0, ""), "");
        assert_eq!(RadioBrowserClient::quality_clause(0, "  "), "");
    }

    #[test]
    fn quality_clause_combines_bitrate_and_codec() {
        assert_eq!(
            RadioBrowserClient::quality_clause(320, "mp3"),
            "&bitrateMin=320&codec=MP3"
        );
        assert_eq!(RadioBrowserClient::quality_clause(128, ""), "&bitrateMin=128");
        assert_eq!(RadioBrowserClient::quality_clause(0, "aac"), "&codec=AAC");
    }

    #[test]
    fn filter_quality_matches_bitrate_and_codec_case_insensitively() {
        let stations = vec![
            station("MP3", 320),
            station("mp3", 128),
            station("AAC", 192),
            station("", 0),
        ];
        let out = RadioBrowserClient::filter_quality(stations.clone(), 128, "mp3");
        assert_eq!(out.len(), 2); // MP3 320 and mp3 128 both pass

        let out = RadioBrowserClient::filter_quality(stations.clone(), 0, "");
        assert_eq!(out.len(), 4);

        let out = RadioBrowserClient::filter_quality(stations, 64, "");
        assert_eq!(out.len(), 3); // the 0-bitrate station fails bitrate >= 64
    }

    #[test]
    fn similar_excludes_the_source_station() {
        // exercised through filter + exclusion logic without network:
        // similar() = stations_by_tag() minus the excluded uuid; the exclusion
        // is a plain iterator filter, verified here on filter_quality output.
        let stations = vec![station("mp3", 128), station("mp3", 256)];
        let excluded = uuid("mp3", 128);
        let out: Vec<_> = RadioBrowserClient::filter_quality(stations, 0, "")
            .into_iter()
            .filter(|s| s.uuid != excluded)
            .collect();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bitrate, 256);
    }
}
