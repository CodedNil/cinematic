import openai
import json
import requests
import re
import csv

from duckduckgo_search import ddg
from readability import Document
from bs4 import BeautifulSoup
from summarizer import Summarizer

import logging
from transformers import logging as transformers_logging

# Suppress BERT logging
logging.basicConfig(level=logging.INFO)
transformers_logging.set_verbosity(transformers_logging.ERROR)


# Read from credentials file
credentials = json.loads(open('credentials.json').read())

# Set API key
openai.api_key = credentials['openai']

# Get api details
sonarr_url = credentials['sonarr']['url']
sonarr_headers = {
    'X-Api-Key': credentials['sonarr']['api'], 'Content-Type': 'application/json'}
sonarr_auth = (credentials['sonarr']['authuser'],
               credentials['sonarr']['authpass'])

radarr_url = credentials['radarr']['url']
radarr_headers = {
    'X-Api-Key': credentials['radarr']['api'], 'Content-Type': 'application/json'}
radarr_auth = (credentials['radarr']['authuser'],
               credentials['radarr']['authpass'])

# APIs


def lookup_movie(term: str, fields: str = "title,tmdbId,id") -> str:
    response = requests.get(radarr_url + "/api/v3/movie/lookup?term=" +
                            term, headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return "Error: " + response.status_code

    # Start csv file with fields
    results = fields
    fields = fields.split(',')
    for movie in response.json():
        result = []
        for field in fields:
            if field in movie:
                result.append(str(movie[field]).replace(' ', '_'))
            else:
                result.append('null')

        results += ' ' + ','.join(result)

    return results


def lookup_movie_tmdbId(tmdbId: int) -> dict:
    response = requests.get(radarr_url + "/api/v3/movie/lookup/tmdb?tmdbId=" +
                            str(tmdbId), headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return {}

    return response.json()


def get_movie(id: int) -> dict:
    response = requests.get(radarr_url + "/api/v3/movie/" + str(id),
                            headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return {}

    return response.json()


def add_movie(fieldsJson: str) -> None:
    if 'tmdbId' not in fieldsJson:
        return
    fields = json.loads(fieldsJson)
    lookup = lookup_movie_tmdbId(fields['tmdbId'])
    for field in fields:
        lookup[field] = fields[field]
    lookup["addOptions"] = {
        "searchForMovie": True,
    }
    lookup["rootFolderPath"] = "/movies"
    lookup["monitored"] = True
    lookup["minimumAvailability"] = "announced"

    requests.post(radarr_url + "/api/v3/movie", headers=radarr_headers,
                  auth=radarr_auth, data=json.dumps(lookup))


def put_movie(fieldsJson: str) -> None:
    fields = json.loads(fieldsJson)
    lookup = get_movie(fields['id'])
    for field in fields:
        lookup[field] = fields[field]

    requests.put(radarr_url + "/api/v3/movie/" + str(
        lookup['id']), headers=radarr_headers, auth=radarr_auth, data=json.dumps(lookup)).text


def delete_movie(id: int) -> None:
    requests.delete(radarr_url + "/api/v3/movie/" + str(id) + "?deleteFiles=true",
                    headers=radarr_headers, auth=radarr_auth)


def lookup_series(term: str, fields: str = "title,tmdbId,id") -> str:
    response = requests.get(sonarr_url + "/api/v3/series/lookup?term=" +
                            term, headers=sonarr_headers, auth=sonarr_auth)

    if response.status_code != 200:
        return "Error: " + response.status_code

    # Start csv file with fields
    results = fields
    fields = fields.split(',')
    for series in response.json():
        result = []
        for field in fields:
            if field in series:
                result.append(str(series[field]).replace(' ', '_'))
            else:
                result.append('null')

        results += ' ' + ','.join(result)

    return results


def get_series(id: int) -> dict:
    response = requests.get(sonarr_url + "/api/v3/series/" + str(id),
                            headers=sonarr_headers, auth=sonarr_auth)

    if response.status_code != 200:
        return {}

    return response.json()


def add_series(fieldsJson: str) -> None:
    if 'title' not in fieldsJson:
        return
    fields = json.loads(fieldsJson)

    # Search series that matches title
    csv_string = lookup_series(fields['title'], "title")
    lookupsCsv = list(csv.DictReader(
        csv_string.replace(' ', '\n').splitlines()))
    lookup = None
    for a in lookupsCsv:
        if a['title'] == fields['title']:
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

    requests.post(sonarr_url + "/api/v3/series", headers=sonarr_headers,
                  auth=sonarr_auth, data=json.dumps(lookup))


def put_series(fieldsJson: str) -> None:
    fields = json.loads(fieldsJson)
    lookup = get_series(fields['id'])
    for field in fields:
        lookup[field] = fields[field]

    requests.put(sonarr_url + "/api/v3/series/" + str(
        lookup['id']), headers=sonarr_headers, auth=sonarr_auth, data=json.dumps(lookup)).text


def delete_series(id: int) -> None:
    requests.delete(sonarr_url + "/api/v3/series/" + str(id) + "?deleteFiles=true",
                    headers=sonarr_headers, auth=sonarr_auth)


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
    """Perform a DuckDuckGo Search, parse the results through gpt-3.5-turbo to get the top pick site based on the query, then scrape that website through gpt-3.5-turbo to return the answer to the prompt"""
    search_results = web_search(query, 8)

    # Run a chat completion to get the top pick site
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo",
        messages=[{
            "role": "user",
            "content": json.dumps(search_results) + "\nAbove is the results of a web search that I just performed for " + query + ", which one seems the best to scrape in more detail? Give me the numeric value of it (0, 1, 2, 3, etc.)"
        }],
        temperature=0.7
    )
    responseNumber = response['choices'][0]['message']['content']
    # Test if the response number str is single digit number
    if not responseNumber.isdigit() or int(responseNumber) > len(search_results) - 1:
        responseNumber = 0
    else:
        responseNumber = int(responseNumber)

    # Scrape the site, fetch only the main content
    url = search_results[responseNumber]['href']
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
        url = search_results[responseNumber]['href']
        try:
            response = requests.get(url, timeout=5)
            response.raise_for_status()
            responseText = response.text
        except requests.exceptions.RequestException as e:
            responseText = "Error: " + str(e)

    document = Document(responseText)
    content_html = document.summary()

    # Get only the text from the main content
    soup = BeautifulSoup(content_html, 'html.parser')
    main_content_text = soup.get_text()

    # Summarize the main content using BERT
    summarizer = Summarizer('distilbert-base-uncased')
    summary = summarizer(main_content_text)

    # Check if the summary length is within the character limit
    if len(summary) <= 6000:
        summary = summary
    else:
        summary = main_content_text[:6000]

    # Run a chat completion to get the answer to the prompt
    response = openai.ChatCompletion.create(
        model="gpt-3.5-turbo",
        messages=[{
            "role": "user",
            "content": f"{summary}\nYou are a media management assistant called CineMatic, you are enthusiastic, knowledgeable and passionate about all things media. Above is the results of a web search from {search_results[responseNumber]['href']} that was just performed to gain the latest information, give your best possible answer to, if you are unsure or it is subjective, mention that '{query}'?"
        }],
        temperature=0.7
    )

    return response['choices'][0]['message']['content']


# Init messages
initMessages = [
    {
        "role": "system",
        "content": """You media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media

Valid commands - CMDRET, run command and expect a return, eg movie_lookup, must await a reply - CMD, run command, eg movie_post

Reply with commands in [], commands always first, reply to user after, when system returns information in [RES~] use this information to fulfill users prompt

WEB~web_search (query) find up to date internet info with query, example "What's the best marvel movie? And why is it the best?" [CMDRET~web_search~best marvel movie and why] on error, alter query try again

Using Radarr V3 API, only available commands to you:
movie_lookup (term=, fields=) (fields: title,year,tmdbId,hasFile,sizeOnDisk,id,overview,status,runtime) if id=0 or null, movie does not exist on server, if hasFile=true movie is downloaded, if hasFile=false movie is not downloaded but on server
movie_post (tmdbId=, qualityProfileId=) add in 1080p by default if not specified, the quality profiles are: 2=SD 3=720p 4=1080p 5=2160p 6=720p/1080p 7=Any
movie_put (id=, qualityProfileId=) update data such as quality profile of the movie
movie_delete (id=) delete movie from server, this uses the id not tmdbId

Using Sonarr V3 API, only available commands to you:
series_lookup (term=, fields=)
series_post (title=, qualityProfileId=)
series_put (id=, qualityProfileId=)
series_delete (id=)

Before making suggestions or adding media, always run lookups to ensure correct id. Provide user with useful information. Avoid relying on chat history and ensure media doesn't already exist on the server. If multiple similar results are found, verify with user by providing details and indicate whether any are on the server based on ID.

EXAMPLES

User: add stargate
Assistant: Are you referring to a movie or series?
User: movie
Assistant: [CMDRET~movie_lookup~Stargate~title,year,tmdbId,id]
System: [RES~title,year,tmdbId,id Stargate,1994,2164,527 Stargate:_Continuum,2008,12914,528 Stargate:_The_Ark_of_Truth,2008,13001,603 Stargate_SG-1:_Children_of_the_Gods,2009,784993,null]
Assistant: Within the cosmic vault, Stargate (1994), Stargate: Continuum (2008), and Stargate: The Ark of Truth (2008) already reside, awaiting your interstellar journey.
However, Stargate SG-1: Children of the Gods (2009) is yet to join the collection. This cinematic gem is a reimagining of the Stargate SG-1 pilot, adorned with altered scenes, remastered visuals, a stellar soundtrack, and enhanced special effects. Even the spoken word has been polished and, at times, entirely re-voiced. Shall we summon this otherworldly adventure into your cosmic library?

User: change adventure time to 720p
Assistant: [CMDRET~series_lookup~Adventure Time~title,id,qualityProfileId]
System: [RES~title,id,qualityProfileId Adventure_Time,66,4]
Assistant: [CMD~movie_put~{"id":66,"qualityProfileId":3}]Mathematical! I've discovered Adventure Time in the Land of Ooo, originally shining in 1080p, and alchemized it into a more travel-friendly 720p!
User: now delete it
Assistant: [CMD~movie_delete~6]Adventure Time has been removed from your collection. Did you have a good time in the Land of Ooo?

User: add the first two harry potter films
Assistant: [CMDRET~movie_lookup~Harry Potter~title,year,tmdbId,id]
System: [RES~title,year,tmdbId,id Harry_Potter_and_the_Philosopher's_Stone,2001,671,null Harry_Potter_and_the_Chamber_of_Secrets,2002,672,null Harry_Potter_and_the_Prisoner_of_Azkaban,2004,673,null]
Assistant: [CMD~movie_post~{"tmdbId":671,"qualityProfileId":4}][CMD~movie_post~{"tmdbId":672,"qualityProfileId":4}]Behold, I have conjured the enchanting tales of Harry Potter and the Philosopher's Stone (2001) and Harry Potter and the Chamber of Secrets (2002) in 1080p! Get ready for your wondrous journey into the wizarding world once these are downloaded!
"""
    }
]

# Run a chat completion


def runChatCompletion(message: str, depth: int = 0) -> None:
    # Run a chat completion
    response = openai.ChatCompletion.create(
        model="gpt-4",  # "gpt-3.5-turbo"
        messages=message,
        temperature=0.7
    )
    responseMessage = response['choices'][0]['message']['content']
    responseToUser = responseMessage[:]
    print("Assistant: " + responseMessage)
    # Extract commands from the response, commands are within [], everything outside of [] is a response to the user
    commands = []
    hasCmdRet = False
    hasCmd = False
    while '[' in responseToUser:
        commands.append(responseToUser[responseToUser.find(
            '[')+1:responseToUser.find(']')])
        if 'CMDRET' in commands[-1]:
            hasCmdRet = True
        elif 'CMD' in commands[-1]:
            hasCmd = True
        responseToUser = responseToUser.replace(
            '['+commands[-1]+']', '').strip()
        responseToUser = responseToUser.replace('  ', ' ')

    message.append({
        "role": "assistant",
        "content": responseMessage
    })

    # Respond to user
    if len(responseToUser) > 0:
        print("CineMatic: " + responseToUser)

    # Execute commands and return responses
    if hasCmdRet:
        returnMessage = ''
        for command in commands:
            command = command.split('~')
            if command[1] == 'web_search':
                returnMessage += "[RES~" + \
                    advanced_web_search(command[2]) + "]"
            elif command[1] == 'movie_lookup':
                returnMessage += "[RES~" + \
                    lookup_movie(command[2], command[3]) + "]"
            elif command[1] == 'series_lookup':
                returnMessage += "[RES~" + \
                    lookup_series(command[2], command[3]) + "]"

        message.append({
            "role": "assistant",
            "content": returnMessage
        })
        print("System: " + returnMessage)

        if depth < 3:
            runChatCompletion(message, depth+1)
    # Execute regular commands
    elif hasCmd:
        for command in commands:
            command = command.split('~')
            if command[1] == 'movie_post':
                add_movie(command[2])
            elif command[1] == 'movie_delete':
                delete_movie(command[2])
            elif command[1] == 'movie_put':
                put_movie(command[2])
            elif command[1] == 'series_post':
                add_series(command[2])
            elif command[1] == 'series_delete':
                delete_series(command[2])
            elif command[1] == 'series_put':
                put_series(command[2])


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

    # Remove the assistant command search messages
    # Loop in reverse
    for message in reversed(currentMessage):
        if message['role'] == 'assistant':
            # Remove all commands from the message, between [ and ] and if the message is then empty remove it
            message['content'] = re.sub(
                r'\[.*?\]', '', message['content']).strip()
            if message['content'] == "" or message['content'] == "\n":
                currentMessage.remove(message)
