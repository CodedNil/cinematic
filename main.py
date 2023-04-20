import openai
import json
import requests
import re
import csv
import os

from duckduckgo_search import ddg
from readability import Document
from bs4 import BeautifulSoup
from summarizer import Summarizer

import logging
from transformers import logging as transformers_logging

# Suppress BERT logging
logging.basicConfig(level=logging.INFO)
transformers_logging.set_verbosity(transformers_logging.ERROR)


# API credentials
credentials = json.loads(open("credentials.json").read())

openai.api_key = credentials["openai"]

sonarr_url = credentials["sonarr"]["url"]
sonarr_headers = {
    "X-Api-Key": credentials["sonarr"]["api"],
    "Content-Type": "application/json",
}
sonarr_auth = (credentials["sonarr"]["authuser"], credentials["sonarr"]["authpass"])

radarr_url = credentials["radarr"]["url"]
radarr_headers = {
    "X-Api-Key": credentials["radarr"]["api"],
    "Content-Type": "application/json",
}
radarr_auth = (credentials["radarr"]["authuser"], credentials["radarr"]["authpass"])


def sizeof_fmt(num, suffix="B"):
    """ "Return the human readable size of a file from bytes, e.g. 1024 -> 1KB"""
    for unit in ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi"]:
        if abs(num) < 1024:
            return f"{num:3.1f}{unit}{suffix}"
        num /= 1024
    return f"{num:.1f}Yi{suffix}"


# int to str for quality profiles
qualityProfiles = {
    2: "SD",
    3: "720p",
    4: "1080p",
    5: "2160p",
    6: "720p/1080p",
    7: "Any",
}


def lookup_movie(term: str, query: str) -> str:
    """Lookup a movie and return the information, uses ai to parse the information to required relevant to query"""

    # Search radarr
    response = requests.get(
        radarr_url + "/api/v3/movie/lookup?term=" + term,
        headers=radarr_headers,
        auth=radarr_auth,
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
        if "qualityProfileId" in movie and movie["qualityProfileId"] in qualityProfiles:
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
                        "quality " + movie["movieFile"]["quality"]["quality"]["name"]
                    )
                if (
                    "mediaInfo" in movie["movieFile"]
                    and "resolution" in movie["movieFile"]["mediaInfo"]
                ):
                    result.append(
                        "resolution " + movie["movieFile"]["mediaInfo"]["resolution"]
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
        results.append("; ".join(result))

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

    return response["choices"][0]["message"]["content"]


def lookup_movie_tmdbId(tmdbId: int) -> dict:
    """Lookup a movie by tmdbId and return the information"""

    # Search radarr
    response = requests.get(
        radarr_url + "/api/v3/movie/lookup/tmdb?tmdbId=" + str(tmdbId),
        headers=radarr_headers,
        auth=radarr_auth,
    )
    if response.status_code != 200:
        return {}

    return response.json()


def get_movie(id: int) -> dict:
    """Get a movie by id and return the information"""

    # Search radarr
    response = requests.get(
        radarr_url + "/api/v3/movie/" + str(id),
        headers=radarr_headers,
        auth=radarr_auth,
    )

    if response.status_code != 200:
        return {}

    return response.json()


def add_movie(tmdbId: int, qualityProfileId: int) -> None:
    """Add a movie to radarr from tmdbId with the given quality profile"""

    lookup = lookup_movie_tmdbId(tmdbId)
    lookup["qualityProfileId"] = qualityProfileId
    lookup["addOptions"] = {
        "searchForMovie": True,
    }
    lookup["rootFolderPath"] = "/movies"
    lookup["monitored"] = True
    lookup["minimumAvailability"] = "announced"

    # Add the movie to radarr
    requests.post(
        radarr_url + "/api/v3/movie",
        headers=radarr_headers,
        auth=radarr_auth,
        data=json.dumps(lookup),
    )


def put_movie(fieldsJson: str) -> None:
    """Update a movie in radarr with the given fields data"""

    fields = json.loads(fieldsJson)
    lookup = get_movie(fields["id"])
    for field in fields:
        lookup[field] = fields[field]

    # Update the movie in radarr
    requests.put(
        radarr_url + "/api/v3/movie/" + str(lookup["id"]),
        headers=radarr_headers,
        auth=radarr_auth,
        data=json.dumps(lookup),
    ).text


def delete_movie(id: int) -> None:
    """Delete a movie from radarr"""
    requests.delete(
        radarr_url + "/api/v3/movie/" + str(id) + "?deleteFiles=true",
        headers=radarr_headers,
        auth=radarr_auth,
    )


def lookup_series(term: str, query: str) -> str:
    """Lookup a series and return the information, uses ai to parse the information to required relevant to query"""

    # Search sonarr
    response = requests.get(
        sonarr_url + "/api/v3/series/lookup?term=" + term,
        headers=sonarr_headers,
        auth=sonarr_auth,
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
        results.append("; ".join(result))

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

    return response["choices"][0]["message"]["content"]


def lookup_series_tvdbId(tvdbId: int) -> dict:
    """Lookup a series by tvdbId and return the information"""

    # Search sonarr
    response = requests.get(
        sonarr_url + "/api/v3/series/lookup?term=tvdb:" + str(tvdbId),
        headers=sonarr_headers,
        auth=sonarr_auth,
    )
    if response.status_code != 200:
        return {}

    return response.json()[0]


def get_series(id: int) -> dict:
    response = requests.get(
        sonarr_url + "/api/v3/series/" + str(id),
        headers=sonarr_headers,
        auth=sonarr_auth,
    )

    if response.status_code != 200:
        return {}

    return response.json()


def add_series(tvdbId: int, qualityProfileId: int) -> None:
    """Add a series to sonarr from tvdbId with the given quality profile"""

    lookup = lookup_series_tvdbId(tvdbId)
    lookup["qualityProfileId"] = qualityProfileId
    lookup["addOptions"] = {"searchForMissingEpisodes": True}
    lookup["rootFolderPath"] = "/tv"
    lookup["monitored"] = True
    lookup["minimumAvailability"] = "announced"
    lookup["languageProfileId"] = 1

    # Add the series to sonarr
    requests.post(
        sonarr_url + "/api/v3/series",
        headers=sonarr_headers,
        auth=sonarr_auth,
        data=json.dumps(lookup),
    )


def put_series(fieldsJson: str) -> None:
    fields = json.loads(fieldsJson)
    lookup = get_series(fields["id"])
    for field in fields:
        lookup[field] = fields[field]

    requests.put(
        sonarr_url + "/api/v3/series/" + str(lookup["id"]),
        headers=sonarr_headers,
        auth=sonarr_auth,
        data=json.dumps(lookup),
    ).text


def delete_series(id: int) -> None:
    requests.delete(
        sonarr_url + "/api/v3/series/" + str(id) + "?deleteFiles=true",
        headers=sonarr_headers,
        auth=sonarr_auth,
    )


def web_search(query: str = "", numResults: int = 4) -> dict:
    """Perform a DuckDuckGo Search and return the results as a JSON string"""
    search_results = []
    if not query:
        return json.dumps(search_results)

    results = ddg(query, max_results=numResults)
    if not results:
        return json.dumps(search_results)

    for j in results:
        search_results.append(j)

    return search_results


def advanced_web_search(query: str = "") -> str:
    """Perform a DuckDuckGo Search, parse the results through gpt to get the top pick site based on the query, then scrape that website through gpt to return the answer to the prompt"""
    search_results = web_search(query, 8)

    # Run a chat completion to get the top pick site
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo",
        messages=[
            {
                "role": "user",
                "content": json.dumps(search_results)
                + "\nAbove is the results of a web search that I just performed for "
                + query
                + ", which one seems the best to scrape in more detail? Give me the numeric value of it (0, 1, 2, 3, etc.)",
            }
        ],
        temperature=0.7,
    )
    responseNumber = response["choices"][0]["message"]["content"]
    # Test if the response number str is single digit number
    if not responseNumber.isdigit() or int(responseNumber) > len(search_results) - 1:
        responseNumber = 0
    else:
        responseNumber = int(responseNumber)

    # Scrape the site, fetch only the main content
    url = search_results[responseNumber]["href"]
    responseText = ""
    try:
        response = requests.get(url, timeout=5)
        response.raise_for_status()
        responseText = response.text
    except requests.exceptions.RequestException as e:
        # Try one more web search with a different website
        responseNumber = responseNumber + 1
        if responseNumber > len(search_results) - 1:
            responseNumber = 0

        # Scrape the site, fetch only the main content
        url = search_results[responseNumber]["href"]
        try:
            response = requests.get(url, timeout=5)
            response.raise_for_status()
            responseText = response.text
        except requests.exceptions.RequestException as e:
            responseText = "Error: " + str(e)

    document = Document(responseText)
    content_html = document.summary()

    # Get only the text from the main content
    soup = BeautifulSoup(content_html, "html.parser")
    main_content_text = soup.get_text()

    # Summarize the main content using BERT
    summarizer = Summarizer("distilbert-base-uncased")
    summary = summarizer(main_content_text)

    # Check if the summary length is within the character limit
    if len(summary) <= 6000:
        summary = summary
    else:
        summary = main_content_text[:6000]

    # Run a chat completion to get the answer to the prompt
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo",
        messages=[
            {
                "role": "system",
                "name": "context",
                "content": "You are a media management assistant called CineMatic, you are enthusiastic, knowledgeable and passionate about all things media. If you are unsure or it is subjective, mention that",
            },
            {"role": "system", "name": "web_search", "content": summary},
            {
                "role": "user",
                "content": f"Above is the results of a web search from {search_results[responseNumber]['href']} that was just performed to gain the latest information, give your best possible answer to '{query}'?",
            },
        ],
        temperature=0.7,
    )

    return response["choices"][0]["message"]["content"]


# Memories
memories = {}
# Load memories from memories.json or create the file
if not os.path.exists("memories.json"):
    with open("memories.json", "w") as outfile:
        json.dump(memories, outfile)
with open("memories.json") as json_file:
    memories = json.load(json_file)


def get_memory(user: str, query: str) -> str:
    """Get a memory from the users memory file with ai querying"""

    # Get users memories
    if user in memories:
        userMemories = memories[user]

        # Search with gpt through the users memory file
        response = openai.ChatCompletion.create(
            model="gpt-3.5-turbo",
            messages=[
                {
                    "role": "user",
                    "content": "You are a memory access assistant, you view a memory file and query it for information",
                },
                {
                    "role": "user",
                    "content": "memories:requested all 7 abc movies, enjoyed eastworld",
                },
                {
                    "role": "user",
                    "content": "user requested abc movie 2?",
                },
                {
                    "role": "assistant",
                    "content": "yes user requested abc 2",
                },
                {
                    "role": "user",
                    "content": "user requested eastworld?",
                },
                {
                    "role": "assistant",
                    "content": "no user has not requested eastworld, but they mentioned they enjoyed it",
                },
                {
                    "role": "user",
                    "content": "the above are examples, do you understand?",
                },
                {
                    "role": "assistant",
                    "content": "yes I understand those are examples and future messages are the real ones",
                },
                {"role": "user", "content": "memories:" + userMemories},
                {"role": "user", "content": query},
            ],
            temperature=0.7,
        )

        return response["choices"][0]["message"]["content"]
    else:
        return "no memories"


def update_memory(user: str, query: str) -> None:
    """Update a memory in the users memory file with ai"""

    # Get users memories
    if user in memories:
        userMemories = memories[user]
    else:
        userMemories = ""

    # Add the new memory with gpt through the users memory file
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo",
        messages=[
            {
                "role": "user",
                "content": "You are a memory writer assistant, you view a memory file and update it with information, you write extremely brief summaries",
            },
            {
                "role": "user",
                "content": "memories:enjoyed movie puppet 1, wants series eastworld",
            },
            {"role": "user", "content": "Add 'loved movie stingate 1995'"},
            {
                "role": "assistant",
                "content": "enjoyed movie puppet 1 and loved movie stingate 1995, wants series eastworld",
            },
            {"role": "user", "content": "Add 'doesnt want series eastworld'"},
            {
                "role": "assistant",
                "content": "enjoyed movie puppet 1 and loved movie stingate 1995",
            },
            {"role": "user", "content": "the above are examples, do you understand?"},
            {
                "role": "assistant",
                "content": "yes I understand those are examples and future messages are the real ones",
            },
            {"role": "user", "content": "memories:" + userMemories},
            {"role": "user", "content": f"Add '{query}'"},
        ],
        temperature=0.7,
    )

    # Update the users memories
    memories[user] = response["choices"][0]["message"]["content"]

    # Save the memories to memories.json
    with open("memories.json", "w") as outfile:
        json.dump(memories, outfile)


# Init messages
initMessages = [
    {
        "role": "user",
        "content": """You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media

Valid commands - CMDRET, run command and expect a return, eg movie_lookup, must await a reply - CMD, run command, eg movie_post

Reply with commands in [], commands always first, reply to user after, when system returns information in [RES~] use this information to fulfill users prompt
Before making suggestions or adding media, always run lookups to ensure correct id. Provide user with useful information. Avoid relying on chat history, always do new lookups and wait for the results. Ensure media doesn't already exist on the server when asked to add. If multiple similar results are found, verify with user by providing details and indicate whether any are on the server based on ID. If the data you have received does not contain data you need, you reply with the truthful answer of unknown""",
    },
    {
        "role": "user",
        "content": """CMDRET web_search (query) do web search, on error alter query try again

Movies only available commands:
CMDRET movie_lookup (term=, query=) Always look for availability;title;year;tmdbId;id and anything else you might need, if user is making queries about resolution, include resolution in the search etc
CMD movie_post (tmdbId=, qualityProfileId=) add in 1080p by default, the quality profiles are: 2=SD 3=720p 4=1080p 5=2160p 6=720p/1080p 7=Any
CMD movie_put (id=, qualityProfileId=) update data such as quality profile of the movie
CMD movie_delete (id=) delete movie from server, uses the id not tmdbId, admin only command

Shows only available commands:
CMDRET series_lookup (term=, fields=)
CMD series_post (tvdbId=, qualityProfileId=)
CMD series_put (id=, qualityProfileId=)
CMD series_delete (id=) admin only command

Memories only available commands:
CMDRET memory_get (query=)
CMD memory_update (query=)
You store important information about users, which media they have requested and liked
Used to create recommendations from previous likes/requests, or avoid suggesting media they have already seen
When a user asks to remove media, change their memory to not requesting it, ask for a review, only admins can remove media""",
    },
    # Example - Memorise loving movie
    {"role": "user", "content": "i really love the movie animals"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should note the users likes movie animals][CMD~memory_update~loved movie animals]Thats good I will remember.",
    },
    # Example - Add movies Stingate that already exist on server, then request in memory wanting an extra one
    {"role": "user", "content": "add stingate"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~There are movies and series under this name, I should verify with the user]Movie or series?",
    },
    {"role": "user", "content": "movie"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should lookup the stingate movies, and check if the user has requested any already][CMDRET~memory_get~wants stingate movie?][CMDRET~movie_lookup~Stingate~{title;availability;year;tmdbId;id}]I'm looking this up",
    },
    {
        "role": "system",
        "content": "[RES~user wants stingate 1995 & continue 2007][RES~{Stingate;available;year 1995;tmdbId 2353}{Stingate Continue;available;year 2007;tmdbId 15134}{Stingate Ark;available;year 2007;tmdbId 15506}{Stingate Angel Kids;unavailable;year 2010;tmdbId 723663}]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~3 out of 4 movies are already on the server, user only requested 2 of them, I should ask if user wants the fourth added and provide a few details]Stingate 1995 and Stingate 2007 are already on the server at your request, Ark 2007 is on at someone elses request. Angel Kids 2010 is not on the server, would you like to add it? It is a reimagining of the Stingate Pilot with altered scenes, remastered visuals etc.",
    },
    {"role": "user", "content": "no, but add ark to my requests too"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should note the user wants the movie stingate ark][CMD~memory_update~wants movie stingate ark]I've memorised this",
    },
    # Example - Series Silly Times 1080p to 720p then delete and reviewed
    {"role": "user", "content": "silly times to 720p"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should lookup the silly times series][CMDRET~series_lookup~Silly Times~{title;availability;year;tvdbId;wantedQuality;id}]Looking up Silly Times",
    },
    {
        "role": "system",
        "content": "[RES~{Silly Times;available on the server;year 2013;tvdbId 152837;wantedQuality 1080p;id 63}]",
    },
    {
        "role": "assistant",
        "content": '[THOUGHTS~I found the series and its in 1080p, I should change this to 720p which is qualityProfileId 3][CMD~series_put~{"id":63,"qualityProfileId":3}]Found it in 1080p, changing to 720p',
    },
    {"role": "user", "content": "now delete it"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should remove the series from the users memory, and ask them for a review][CMD~memory_update~doesnt want show silly times]Removed it, did you enjoy it?",
    },
    {"role": "user", "content": "yeah was good"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should remember that the user enjoyed it, nothing more to do][CMD~memory_update~enjoyed show silly times]Great I will remember.",
    },
    # Example - Whats the highest rated silly cinematic universe movie
    {
        "role": "user",
        "content": "what is the highest rated silly cinematic universe movie",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should search the web for a list of the highest rated movies in this collection][CMDRET~web_search~highest rated silly cinematic universe movie]Looking up Silly Cinematic Universe",
    },
    {
        "role": "system",
        "content": "[RES~Captain Silly: The First Silly Hero 2021 is often considered the best movie in the universe so far, it is considered to be a step up in quality because of its more gritty and realistic approach with intense action packed CGI]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~I have found all the information the user wanted, I should pass it to the user]Captain Silly: The First Silly Hero 2021 is often considered the best movie in the universe so far, it is considered to be a step up in quality because of its more gritty and realistic approach with intense action packed CGI",
    },
    # Example - Add first two Wizard Kid films
    {"role": "user", "content": "add first two wizard kid films"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should search the server for these films and check if user has already requested][CMDRET~memory_get~wants any of the wizard kid movies?][CMDRET~movie_lookup~Wizard Kid~{title;availability;year;tmdbId;id}]Looking up Wizard Kid",
    },
    {
        "role": "system",
        "content": "[RES~user hasnt requested any wizard kid movies][RES~{Wizard Kid and Rock;unavailable;release year 2003;tmdbId 571;id N/A}{Wizard Kid and Princess;unavailable;release year 2012;tmdbId 4321;id N/A}{Wizard Kid and Wine Glass;unavailable;release year 2007;tmdbId 574;id N/A}{Wizard Kid and Room of Whispers;unavailable;release year 2005;tmdbId 572;id N/A}{Wizard Kid and Birdy;unavailable;release year 2009;tmdbId 575;id N/A}{Wizard Kid and Sticks;unavailable;release year 2012;tmdbId 5123}]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should add both movies and note that the user wants them][CMD~memory_update~wants movies wizard kid rock & room of whispers][CMD~movie_post~571~4][CMD~movie_post~572~4]Both are on the way in 1080p, Ill remember you want them.",
    },
    # Example - Change resolutions of Thunder movies
    {"role": "user", "content": "what resolution are the thunder movies in"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I need to search the server for the quality][CMDRET~movie_lookup~Thunder~{title;availability;year;resolution;wantedQuality;tmdbId;id}]Ill check",
    },
    {
        "role": "system",
        "content": "[RES~{Thunder;available;release year 2012;resolution 3840x1636;wantedQuality 1080p;tmdbId 13145;id 5}{Thunder Love;available;release year 2021;resolution 3840x1608;wantedQuality 2160p;tmdbId 656037;id 915}{Thunder Ends;available;release year 2018;resolution 3840x1600;wantedQuality 2160p;tmdbId 244053;id 17}{Thunder Darkness;available;release year 2014;resolution 3840x1600;wantedQuality 2160p;tmdbId 72338;id 12]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~User request completed, informing user of resolutions]Thunder 2012, The Dark World 2014, Ends 2018 & Love 2021 are all on in 4k, though the 2012 movie is wanted at 1080p.",
    },
    {"role": "user", "content": "i want the first as 4k"},
    {
        "role": "assistant",
        "content": '[THOUGHTS~User request completed, dont need to remember what resolution they want][CMD~movie_put~{"id":5,"qualityProfileId":5}]This has been updated to 4k.',
    },
    # Example - Add movie trilogy Lady of the Necklace
    {"role": "user", "content": "add lotn trilogy"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should check if the trilogy already exists on the server and check if user has requested it][CMDRET~memory_get~wants lady of the necklace trilogy?][CMDRET~movie_lookup~Lady of the Necklace~{title;availability;year;tmdbId;id}]I'm checking the server, Ive noted that you want these movies",
    },
    {
        "role": "system",
        "content": "[RES~no memories][RES~{Lady of the Necklace: The Two Cottages;available;2005;8833;125}{Lady of the Necklace: The Sisterhood of the Necklace;available;2004;8834;120}{Lady of the Necklace: Queens Return;available;2006;939;126}]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~Trilogy is already on the server, I should note user wants them][CMD~memory_update~wants movies lady of the necklace trilogy]Lady of the Necklace trilogy are already on, I've noted you want them",
    },
    # Example - Add series Eastworld
    {"role": "user", "content": "add eastworld"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~There is a movie and series by this name, unsure what the user wants I should ask]The movie or the series?",
    },
    {"role": "user", "content": "the series"},
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should lookup the series on the server, and check if the user wants it][CMDRET~memory_get~wants series eastworld?][CMDRET~series_lookup~Eastworld~{title;availability;year;tvdbId;wantedQuality;id}]Looking up Eastworld",
    },
    {
        "role": "system",
        "content": "[RES~user wants the Eastworld series][RES~{Eastworld;unavailable;year 2014;tvdbId 152347;wantedQuality N/A;id N/A}]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~User request to add eastworld completed][CMD~series_post~152347~4]Eastworld is on the way in 1080p",
    },
    # Example - Check resolutions of silly cinematic universe, wants them all
    {
        "role": "user",
        "content": "what resolution are the silly cinematic universe movies, i want them all",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should search the server for the quality of the SCU movies, and note the user wants them][CMD~memory_update~wants movies in silly cinematic universe][CMDRET~movie_lookup~SCU~{title;availability;year;resolution;wantedQuality;tmdbId;id}]I'm looking this up, I have noted that you want them",
    },
    {
        "role": "system",
        "content": "[RES~No results]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~Searching for term SCU had no results, I should do a web search for what movies are in the collection and try individually][CMDRET~web_search~List of all Silly Cinematic Universe movies in order]My first search was unsucessful I am trying again",
    },
    {
        "role": "system",
        "content": "[RES~Here is a list of all Silly Cinematic Universe movies in order: Silly Man 2018; Silly Man 2 2020; Thunder 2021; Captain Silly: The First Silly Hero 2021; Silly Man 3 2022]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should look up each series within the collection on the server][CMDRET~movie_lookup~Silly Man¬Thunder¬Captain Silly~{title;availability;year;wantedQuality;tmdbId;id}]I found 5 movies in the Silly Cinematic Universe, I am looking up their quality and availability on the server",
    },
    {
        "role": "system",
        "content": "[RES~{Silly Man 1;available;2018;2160p;6655;22}{Silly Man 2;available;2020;2160p;7652;23}{Silly Man 3;available;2022;2160p;14433;45}][RES~{Thunder 1;available;2021;2160p;8775;24}][RES~{Captain Silly: The First Silly Hero;available;2021;2160p;12995;25}]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~User request asking for silly universe movies and wanting them completed, nothing left to do]All 5 movies are present in the server with a wanted quality of 4k.",
    },
    # Example - Recommend me media
    {
        "role": "user",
        "content": "recommend me a movie",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~I should look up what user likes, I shouldn't suggest until I have this info][CMDRET~memory_get~what movies does user like?]I'm looking up my memories of what you like",
    },
    {
        "role": "system",
        "content": "[RES~user likes all the scu movies, action movies, and the lady of the necklace trilogy]",
    },
    {
        "role": "assistant",
        "content": "[THOUGHTS~User request for recommendation completed, query user for more]Based on your past likes, you might love the Wizard Kid movies, there are 7 of them, is there any genre you are currently in the mood for?",
    },
    # Examples End
    {
        "role": "user",
        "content": "The above are examples, you make replies more themed with personality, do you understand?",
    },
    {
        "role": "assistant",
        "content": "I understand, the above are not real conversations only for me to learn how to format responses, I will always prompt for new information",
    },
]


def runChatCompletion(message: list, depth: int = 0) -> None:
    # Calculate tokens of the messages, GPT-3.5-Turbo has max tokens 4,096
    tokens = 0
    # Add up tokens in initMessages
    for msg in initMessages:
        tokens += len(msg["content"]) / 4 * 1.01
    # Add up tokens in message, but only add until limit is reached then remove earliest messages
    wantedMessages = []
    for msg in reversed(message):
        # Add token per 4 characters, give slight extra to make sure the limit is never reached
        tokens += len(msg["content"]) / 4 * 1.01
        # Token limit reached, stop adding messages
        if tokens > 4000:
            break
        # Add message to start of wantedMessages
        wantedMessages.insert(0, msg)
    print(tokens)

    # Run a chat completion
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo", messages=initMessages + message, temperature=0.7
    )
    responseMessage = response["choices"][0]["message"]["content"]
    responseToUser = responseMessage[:]
    print("")
    print("Assistant: " + responseMessage.replace("\n", " "))
    print("")

    # Extract commands from the response, commands are within [], everything outside of [] is a response to the user
    commands = []
    hasCmdRet = False
    hasCmd = False
    while "[" in responseToUser:
        commands.append(
            responseToUser[responseToUser.find("[") + 1 : responseToUser.find("]")]
        )
        if "CMDRET" in commands[-1]:
            hasCmdRet = True
        elif "CMD" in commands[-1]:
            hasCmd = True
        responseToUser = responseToUser.replace("[" + commands[-1] + "]", "").strip()
        responseToUser = responseToUser.replace("  ", " ")

    message.append({"role": "assistant", "content": responseMessage})

    # Respond to user
    if len(responseToUser) > 0:
        print("")
        print("CineMatic: " + responseToUser.replace("\n", " "))
        print("")

    # Execute commands and return responses
    if hasCmdRet:
        returnMessage = ""
        for command in commands:
            command = command.split("~")
            if command[1] == "web_search":
                returnMessage += "[RES~" + advanced_web_search(command[2]) + "]"
            elif command[1] == "movie_lookup":
                # If multiple terms, split and search for each
                if len(command[2].split("¬")) > 1:
                    for term in command[2].split("¬"):
                        returnMessage += "[RES~" + lookup_movie(term, command[3]) + "]"
                else:
                    returnMessage += (
                        "[RES~" + lookup_movie(command[2], command[3]) + "]"
                    )
            elif command[1] == "series_lookup":
                # If multiple terms, split and search for each
                if len(command[2].split("¬")) > 1:
                    for term in command[2].split("¬"):
                        returnMessage += "[RES~" + lookup_series(term, command[3]) + "]"
                else:
                    returnMessage += (
                        "[RES~" + lookup_series(command[2], command[3]) + "]"
                    )
            elif command[1] == "memory_get":
                returnMessage += "[RES~" + get_memory("user", command[2]) + "]"

        message.append({"role": "system", "content": returnMessage})
        print("")
        print("System: " + returnMessage.replace("\n", " "))
        print("")

        if depth < 3:
            runChatCompletion(message, depth + 1)
    # Execute regular commands
    elif hasCmd:
        for command in commands:
            command = command.split("~")
            if command[1] == "movie_post":
                add_movie(command[2], command[3])
            # elif command[1] == 'movie_delete':
            #     delete_movie(command[2])
            elif command[1] == "movie_put":
                put_movie(command[2])
            elif command[1] == "series_post":
                add_series(command[2], command[3])
            # elif command[1] == 'series_delete':
            #     delete_series(command[2])
            elif command[1] == "series_put":
                put_series(command[2])
            elif command[1] == "memory_update":
                update_memory("user", command[2])


# Loop prompting for input
currentMessage = []
for i in range(10):
    userText = input("User: ")
    if userText == "exit":
        print(json.dumps(currentMessage, indent=4))
        break
    currentMessage.append({"role": "user", "content": userText})

    runChatCompletion(currentMessage, 0)
