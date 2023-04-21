import requests
import json
import openai
from modules.module_logs import ModuleLogs

# int to str for quality profiles
qualityProfiles = {
    2: "SD",
    3: "720p",
    4: "1080p",
    5: "2160p",
    6: "720p/1080p",
    7: "Any",
}


def sizeof_fmt(num, suffix="B"):
    """ "Return the human readable size of a file from bytes, e.g. 1024 -> 1KB"""
    for unit in ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi"]:
        if abs(num) < 1024:
            return f"{num:3.1f}{unit}{suffix}"
        num /= 1024
    return f"{num:.1f}Yi{suffix}"


class SeriesAPI:
    """Class to handle Sonarr API calls"""

    def __init__(
        self, openai_key: str, sonarr_url: str, sonarr_headers: dict, sonarr_auth: tuple
    ) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key
        self.sonarr_url = sonarr_url
        self.sonarr_headers = sonarr_headers
        self.sonarr_auth = sonarr_auth

        # Create logs
        self.logs = ModuleLogs("series")

    def lookup_series(self, term: str, query: str) -> str:
        """Lookup a series and return the information, uses ai to parse the information to required relevant to query"""

        # Search sonarr
        response = requests.get(
            self.sonarr_url + "/api/v3/series/lookup?term=" + term,
            headers=self.sonarr_headers,
            auth=self.sonarr_auth,
        )
        if response.status_code != 200:
            return "Error: " + response.status_code

        # If no results, return
        if len(response.json()) == 0:
            return "No results"

        # Convert to plain english
        results = []
        for series in response.json():
            result = []
            # Basic info
            result.append(series["title"])
            result.append("status " + series["status"] + " year " + str(series["year"]))
            if "id" in series and series["id"] != 0:
                result.append("available on the server")
                result.append("id " + str(series["id"]))
            else:
                result.append("unavailable on the server")
            if (
                "qualityProfileId" in series
                and series["qualityProfileId"] in qualityProfiles
            ):
                result.append(
                    "quality wanted " + qualityProfiles[series["qualityProfileId"]]
                )
            if "tvdbId" in series:
                result.append("tvdbId " + str(series["tvdbId"]))
            # Extra info
            if "runtime" in series:
                result.append("runtime " + str(series["runtime"]))
            if "airTime" in series:
                result.append("airTime " + str(series["airTime"]))
            if "network" in series:
                result.append("network " + str(series["network"]))
            if "certification" in series:
                result.append("certification " + str(series["certification"]))
            if "genre" in series:
                result.append("genres " + ", ".join(series["genres"]))
            # Add to results
            results.append(";".join(result))

            # Only include first 10 results
            if len(results) >= 10:
                break

        # Run a chat completion to query the information
        response = openai.ChatCompletion.create(
            model="gpt-3.5-turbo",
            messages=[
                {
                    "role": "user",
                    "content": "You are a data parser assistant, provide a lot of information, if there are multiple matches to the query list them all, you also include data for media not available on the server. Provide a concise summary, format like this with key value {Series_Name;unavailable;release 1995;tvdbId 862}",
                },
                {"role": "user", "content": "\n".join(results)},
                {
                    "role": "user",
                    "content": f"From the above information for term {term}. {query}",
                },
            ],
            temperature=0.7,
        )
        # Log the response
        self.logs.log(
            "query_lookup", " - ".join(results).replace("\n", " "), query, response
        )

        return response["choices"][0]["message"]["content"]

    def lookup_series_tvdbId(self, tvdbId: int) -> dict:
        """Lookup a series by tvdbId and return the information"""

        # Search sonarr
        response = requests.get(
            self.sonarr_url + "/api/v3/series/lookup?term=tvdb:" + str(tvdbId),
            headers=self.sonarr_headers,
            auth=self.sonarr_auth,
        )
        if response.status_code != 200:
            return {}

        return response.json()[0]

    def get_series(self, id: int) -> dict:
        response = requests.get(
            self.sonarr_url + "/api/v3/series/" + str(id),
            headers=self.sonarr_headers,
            auth=self.sonarr_auth,
        )

        if response.status_code != 200:
            return {}

        return response.json()

    def add_series(self, tvdbId: int, qualityProfileId: int) -> None:
        """Add a series to sonarr from tvdbId with the given quality profile"""

        lookup = self.lookup_series_tvdbId(tvdbId)
        lookup["qualityProfileId"] = qualityProfileId
        lookup["addOptions"] = {"searchForMissingEpisodes": True}
        lookup["rootFolderPath"] = "/tv"
        lookup["monitored"] = True
        lookup["minimumAvailability"] = "announced"
        lookup["languageProfileId"] = 1

        # Add the series to sonarr
        requests.post(
            self.sonarr_url + "/api/v3/series",
            headers=self.sonarr_headers,
            auth=self.sonarr_auth,
            data=json.dumps(lookup),
        )

    def put_series(self, fieldsJson: str) -> None:
        fields = json.loads(fieldsJson)
        lookup = self.get_series(fields["id"])
        for field in fields:
            lookup[field] = fields[field]

        requests.put(
            self.sonarr_url + "/api/v3/series/" + str(lookup["id"]),
            headers=self.sonarr_headers,
            auth=self.sonarr_auth,
            data=json.dumps(lookup),
        )

    def delete_series(self, id: int) -> None:
        requests.delete(
            self.sonarr_url + "/api/v3/series/" + str(id) + "?deleteFiles=true",
            headers=self.sonarr_headers,
            auth=self.sonarr_auth,
        )
