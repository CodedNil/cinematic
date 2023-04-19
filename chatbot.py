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

    # Convert to plain english
    results = []
    for movie in response.json():
        result = []
        # Basic info
        result.append(movie["title"])
        result.append("status " + movie["status"] + " year " + str(movie["year"]))
        if "id" in movie and movie["id"] != 0:
            result.append("available on the server")
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


# Tests
# print(lookup_movie("The Matrix", "What is the file size of the matrix?"))
# print(
#     lookup_movie(
#         "Stargate",
#         "List stargate movies with data {availability, title, year, tmdbId}",
#     )
# )
# print(
#     lookup_movie(
#         "Harry Potter ",
#         "List harry potter movies with data {availability, title, year, tmdbId}",
#     )
# )
# print(
#     lookup_movie(
#         "Lord of the Rings ",
#         "List lord of the rings movies with data {availability, title, year, tmdbId}",
#     )
# )


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


def add_movie(fieldsJson: str) -> None:
    """Add a movie to radarr with the given fields data"""

    if "tmdbId" not in fieldsJson:
        return
    fields = json.loads(fieldsJson)
    lookup = lookup_movie_tmdbId(fields["tmdbId"])
    for field in fields:
        lookup[field] = fields[field]
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

    # Convert to plain english
    results = []
    for series in response.json():
        result = []
        # Basic info
        result.append(series["title"])
        result.append("status " + series["status"] + " year " + str(series["year"]))
        if "id" in series and series["id"] != 0:
            result.append("available on the server")
        else:
            result.append("unavailable on the server")
        if (
            "qualityProfileId" in series
            and series["qualityProfileId"] in qualityProfiles
        ):
            result.append(
                "quality wanted " + qualityProfiles[series["qualityProfileId"]]
            )
        if "tmdbId" in series:
            result.append("tmdbId " + str(series["tmdbId"]))
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

    # Run a chat completion to query the information
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo",
        messages=[
            {
                "role": "user",
                "content": "You are a data parser assistant, provide a lot of information, if there are multiple matches to the query list them all, you also include data for media not available on the server. Provide a concise summary, format like this with key value {Series_Name;unavailable;release 1995;tmdbId 862}",
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


# Tests
# print(
#     lookup_series(
#         "Stargate", "List stargate series with data {availability, title, year, tmdbId}"
#     )
# )
# print(
#     lookup_series(
#         "Adventure Time",
#         "List adventure time series with data {availability, title, year, tmdbId, resolution}",
#     )
# )


def get_series(id: int) -> dict:
    response = requests.get(
        sonarr_url + "/api/v3/series/" + str(id),
        headers=sonarr_headers,
        auth=sonarr_auth,
    )

    if response.status_code != 200:
        return {}

    return response.json()


def add_series(fieldsJson: str) -> None:
    if "title" not in fieldsJson:
        return
    fields = json.loads(fieldsJson)

    # Search series that matches title
    csv_string = lookup_series(fields["title"], "title")
    lookupsCsv = list(csv.DictReader(csv_string.replace(" ", "\n").splitlines()))
    lookup = None
    for a in lookupsCsv:
        if a["title"] == fields["title"]:
            lookup = a
            break

    if lookup is None:
        return

    for field in fields:
        lookup[field] = fields[field]
    lookup["addOptions"] = {
        "searchForMissingEpisodes": True,
    }
    lookup["rootFolderPath"] = "/tv"
    lookup["monitored"] = True
    lookup["minimumAvailability"] = "announced"

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
                    "content": "memories:requested all 7 abc movies",
                },
                {
                    "role": "user",
                    "content": "user requested abc movie 2?",
                },
                {
                    "role": "assistant",
                    "content": "yes user requested harry potter deathly hallows part 2",
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
            {"role": "user", "content": "memories:enjoyed avatar 1"},
            {"role": "user", "content": "Add 'loved stargate 1994'"},
            {
                "role": "assistant",
                "content": "enjoyed avatar 1 and loved stargate 1994",
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
Before making suggestions or adding media, always run lookups to ensure correct id. Provide user with useful information. Avoid relying on chat history and ensure media doesn't already exist on the server. If multiple similar results are found, verify with user by providing details and indicate whether any are on the server based on ID.""",
    },
    {
        "role": "user",
        "content": """WEB~web_search (query) do web search, example "Whats top marvel movie?" [CMDRET~web_search~highest rated marvel movie] on error, alter query try again

Movies only available commands:
movie_lookup (term=, query=)
movie_post (tmdbId=, qualityProfileId=) add in 1080p by default, the quality profiles are: 2=SD 3=720p 4=1080p 5=2160p 6=720p/1080p 7=Any
movie_put (id=, qualityProfileId=) update data such as quality profile of the movie
movie_delete (id=) delete movie from server, uses the id not tmdbId, admin only command

Shows only available commands:
series_lookup (term=, fields=)
series_post (title=, qualityProfileId=)
series_put (id=, qualityProfileId=)
series_delete (id=) admin only command

Memories only available commands:
memory_get (query=)
memory_update (query=)
You store important information about users, which media they have requested and liked
Used to create recommendations from previous likes/requests, or avoid suggesting media they have already seen
When a user asks to remove media, change their memory to not requesting it, ask for a review, only admins can remove media""",
    },
    {"role": "user", "content": "i really love the movie cats"},
    {
        "role": "assistant",
        "content": "[CMD~memory_update~loved movie cats]Thats good I will remember.",
    },
    {"role": "user", "content": "add stargate"},
    {"role": "assistant", "content": "Movie or series?"},
    {"role": "user", "content": "movie"},
    {
        "role": "assistant",
        "content": "[CMDRET~memory_get~wants stargate movie?][CMDRET~movie_lookup~Stargate~List stargate movies with data {availability, title, year, tmdbId}]I'm looking this up",
    },
    {
        "role": "system",
        "content": "[RES~user wants stargate 1994 & continuum 2008][RES~Stargate; available; year 1994; tmdbId 2164\Stargate: Continuum; available; year 2008; tmdbId 12914\Stargate: The Ark of Truth; available; year 2008; tmdbId 13001\Stargate SG-1: Children of the Gods; unavailable; year 2009; tmdbId 784993]",
    },
    {
        "role": "assistant",
        "content": "Stargate 1994 and Continuum 2008 are already on the server at your request, Ark of Truth 2008 is on by someone elses request. Children of the Gods 2009 is not on the server, would you like to add it? It is a reimagining of the SG-1 pilot with altered scenes, remastered visuals etc.",
    },
    {"role": "user", "content": "no, but add ark of truth to my requests too"},
    {
        "role": "assistant",
        "content": "[CMD~memory_update~wants movie stargate ark of truth]I've memorised this",
    },
    {"role": "user", "content": "adventure time to 720p"},
    {
        "role": "assistant",
        "content": "[CMDRET~series_lookup~Adventure Time~List adventure time series with data {availability, title, year, tmdbId, resolution}]Looking up Adventure Time",
    },
    {
        "role": "system",
        "content": "[RES~Adventure Time; available on the server; year 2010; tmdbId 15260; resolution 1080p\Adventure Time: Fionna and Cake; unavailable on the server; year 0; tmdbId N/A; resolution N/A]",
    },
    {
        "role": "assistant",
        "content": '[CMD~series_put~{"id":66,"qualityProfileId":3}]Found it in 1080p, changing to 720p',
    },
    {"role": "user", "content": "now delete it"},
    {
        "role": "assistant",
        "content": "[CMD~memory_update~doesnt want show adventure time]Removed it, did you enjoy it?",
    },
    {"role": "user", "content": "yeah was good"},
    {
        "role": "assistant",
        "content": "[CMD~memory_update~enjoyed show adventure time]Great I will remember.",
    },
    {"role": "user", "content": "add first two harry potter films"},
    {
        "role": "assistant",
        "content": "[CMDRET~memory_get~wants first two harry potter movies?][CMDRET~movie_lookup~Harry Potter~List harry potter movies with data {availability, title, year, tmdbId}]Looking up Harry Potter",
    },
    {
        "role": "system",
        "content": "[RES~user hasnt requested any harry potter movies][RES~Harry Potter and the Philosopher's Stone; unavailable; release year 2001; tmdbId 671\Harry Potter and the Half-Blood Prince; unavailable; release year 2009; tmdbId 767\Harry Potter and the Goblet of Fire; unavailable; release year 2005; tmdbId 674\Harry Potter and the Chamber of Secrets; unavailable; release year 2002; tmdbId 672\Harry Potter and the Prisoner of Azkaban; unavailable; release year 2004; tmdbId 673\Harry Potter and the Deathly Hallows: Part 2; unavailable; release year 2011; tmdbId 12445\Harry Potter and the Order of the Phoenix; unavailable; release year 2007; tmdbId 675\Harry Potter and the Deathly Hallows: Part 1; unavailable; release year 2010; tmdbId 12444]",
    },
    {
        "role": "assistant",
        "content": '[CMD~memory_update~wants movies harry potter philosophers stone, chamber of secret][CMD~movie_post~{"tmdbId":671,"qualityProfileId":4}][CMD~movie_post~{"tmdbId":672,"qualityProfileId":4}]Both are on the way in 1080p, Ill remember you want them.',
    },
    {"role": "user", "content": "add lotr trilogy"},
    {
        "role": "assistant",
        "content": "[CMDRET~memory_get~wants lord of the rings trilogy?][CMDRET~movie_lookup~Lord of the Rings~List lord of the rings movies with data {availability, title, year, tmdbId}",
    },
    {
        "role": "system",
        "content": "[RES~no memories][{available on the server, The Lord of the Rings: The Two Towers, 2002, 121}\{available on the server, The Lord of the Rings: The Fellowship of the Ring, 2001, 120}\{available on the server, The Lord of the Rings: The Return of the King, 2003, 122}\{unavailable on the server, The Lord of the Rings, 1978, 123}]",
    },
    {
        "role": "assistant",
        "content": "[CMD~memory_update~wants movies lord of the rings trilogy]The Lord of the Rings trilogy are already on, I have noted you want them.",
    },
    {
        "role": "user",
        "content": "The above are examples, you make replies more themed with personality, do you understand?",
    },
    {
        "role": "assistant",
        "content": "I understand, the above are not real conversations only for me to learn how to format responses",
    },
]


def runChatCompletion(message: str, depth: int = 0) -> None:
    # Run a chat completion
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo", messages=message, temperature=0.7
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
                returnMessage += "[RES~" + lookup_movie(command[2], command[3]) + "]"
            elif command[1] == "series_lookup":
                returnMessage += "[RES~" + lookup_series(command[2], command[3]) + "]"
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
            if command[1] == 'movie_post':
                add_movie(command[2])
            # elif command[1] == 'movie_delete':
            #     delete_movie(command[2])
            elif command[1] == "movie_put":
                put_movie(command[2])
            elif command[1] == 'series_post':
                add_series(command[2])
            # elif command[1] == 'series_delete':
            #     delete_series(command[2])
            elif command[1] == "series_put":
                put_series(command[2])
            elif command[1] == "memory_update":
                update_memory("user", command[2])


# Loop prompting for input
currentMessage = initMessages.copy()
for i in range(10):
    userText = input("User: ")
    if userText == 'exit':
        print(json.dumps(currentMessage, indent=4))
        break
    currentMessage.append({
        "role": "user",
        "content": userText
    })

    runChatCompletion(currentMessage, 0)