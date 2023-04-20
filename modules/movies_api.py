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


class MoviesAPI:
    """Class to handle Radarr API calls"""

    def __init__(
        self, openai_key: str, radarr_url: str, radarr_headers: dict, radarr_auth: tuple
    ) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key
        self.radarr_url = radarr_url
        self.radarr_headers = radarr_headers
        self.radarr_auth = radarr_auth

        # Create logs
        self.logs = ModuleLogs("movies")

    def lookup_movie(self, term: str, query: str) -> str:
        """Lookup a movie and return the information, uses ai to parse the information to required relevant to query"""

        # Search radarr
        response = requests.get(
            self.radarr_url + "/api/v3/movie/lookup?term=" + term,
            headers=self.radarr_headers,
            auth=self.radarr_auth,
        )
        if response.status_code != 200:
            return "Error: " + response.status_code

        # If no results, return
        if len(response.json()) == 0:
            return "No results"

        # Convert to plain english
        results = []
        for movie in response.json():
            result = []
            # Basic info
            result.append(movie["title"])
            result.append("status " + movie["status"] + " year " + str(movie["year"]))
            if "id" in movie and movie["id"] != 0:
                result.append("available on the server")
                result.append("id " + str(movie["id"]))
            else:
                result.append("unavailable on the server")
            if (
                "qualityProfileId" in movie
                and movie["qualityProfileId"] in qualityProfiles
            ):
                result.append(
                    "quality wanted " + qualityProfiles[movie["qualityProfileId"]]
                )
            if "tmdbId" in movie:
                result.append("tmdbId " + str(movie["tmdbId"]))
            # File info
            if "hasFile" in movie and movie["hasFile"] == True:
                result.append("file size " + sizeof_fmt(movie["sizeOnDisk"]))
                if "movieFile" in movie:
                    if (
                        "quality" in movie["movieFile"]
                        and "quality" in movie["movieFile"]["quality"]
                        and "name" in movie["movieFile"]["quality"]["quality"]
                    ):
                        result.append(
                            "quality "
                            + movie["movieFile"]["quality"]["quality"]["name"]
                        )
                    if (
                        "mediaInfo" in movie["movieFile"]
                        and "resolution" in movie["movieFile"]["mediaInfo"]
                    ):
                        result.append(
                            "resolution "
                            + movie["movieFile"]["mediaInfo"]["resolution"]
                        )
                    if "languages" in movie["movieFile"]:
                        languages = []
                        for language in movie["movieFile"]["languages"]:
                            languages.append(language["name"])
                        result.append("languages " + ", ".join(languages))
                    if (
                        "edition" in movie["movieFile"]
                        and movie["movieFile"]["edition"] != ""
                    ):
                        result.append("edition " + movie["movieFile"]["edition"])
            else:
                result.append("no file on disk")
            # Extra info
            if "runtime" in movie:
                result.append("runtime " + str(movie["runtime"]) + " minutes")
            if "certification" in movie:
                result.append("certification " + movie["certification"])
            if "genre" in movie:
                result.append("genres " + ", ".join(movie["genres"]))
            # if 'overview' in movie:
            #     result.append('overview ' + movie['overview'])
            if "studio" in movie:
                result.append("studio " + movie["studio"])
            if "ratings" in movie:
                ratings = []
                for site in movie["ratings"]:
                    ratings.append(
                        site
                        + " rated "
                        + str(movie["ratings"][site]["value"])
                        + " with "
                        + str(movie["ratings"][site]["votes"])
                        + " votes"
                    )
                result.append("ratings " + ", ".join(ratings))
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
                    "content": "You are a data parser assistant, provide a lot of information, if there are multiple matches to the query list them all, you also include data for media not available on the server. Provide a concise summary, format like this with key value {Movie_Name;unavailable;release 1995;tmdbId 862}",
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

    def lookup_movie_tmdbId(self, tmdbId: int) -> dict:
        """Lookup a movie by tmdbId and return the information"""

        # Search radarr
        response = requests.get(
            self.radarr_url + "/api/v3/movie/lookup/tmdb?tmdbId=" + str(tmdbId),
            headers=self.radarr_headers,
            auth=self.radarr_auth,
        )
        if response.status_code != 200:
            return {}

        return response.json()

    def get_movie(self, id: int) -> dict:
        """Get a movie by id and return the information"""

        # Search radarr
        response = requests.get(
            self.radarr_url + "/api/v3/movie/" + str(id),
            headers=self.radarr_headers,
            auth=self.radarr_auth,
        )

        if response.status_code != 200:
            return {}

        return response.json()

    def add_movie(self, tmdbId: int, qualityProfileId: int) -> None:
        """Add a movie to radarr from tmdbId with the given quality profile"""

        lookup = self.lookup_movie_tmdbId(tmdbId)
        lookup["qualityProfileId"] = qualityProfileId
        lookup["addOptions"] = {
            "searchForMovie": True,
        }
        lookup["rootFolderPath"] = "/movies"
        lookup["monitored"] = True
        lookup["minimumAvailability"] = "announced"

        # Add the movie to radarr
        requests.post(
            self.radarr_url + "/api/v3/movie",
            headers=self.radarr_headers,
            auth=self.radarr_auth,
            data=json.dumps(lookup),
        )

    def put_movie(self, fieldsJson: str) -> None:
        """Update a movie in radarr with the given fields data"""

        fields = json.loads(fieldsJson)
        lookup = self.get_movie(fields["id"])
        for field in fields:
            lookup[field] = fields[field]

        # Update the movie in radarr
        requests.put(
            self.radarr_url + "/api/v3/movie/" + str(lookup["id"]),
            headers=self.radarr_headers,
            auth=self.radarr_auth,
            data=json.dumps(lookup),
        ).text

    def delete_movie(self, id: int) -> None:
        """Delete a movie from radarr"""
        requests.delete(
            self.radarr_url + "/api/v3/movie/" + str(id) + "?deleteFiles=true",
            headers=self.radarr_headers,
            auth=self.radarr_auth,
        )