import openai
import json
import requests

# Read from credentials file
credentials = json.loads(open('credentials.json').read())

# Set API key
openai.api_key = credentials['openai']

# Get api details
sonarr_url = credentials['sonarr']['url']
sonarr_headers = {'X-Api-Key': credentials['sonarr']['api']}
sonarr_auth = (credentials['sonarr']['authuser'],
               credentials['sonarr']['authpass'])

radarr_url = credentials['radarr']['url']
radarr_headers = {'X-Api-Key': credentials['radarr']['api']}
radarr_auth = (credentials['radarr']['authuser'],
               credentials['radarr']['authpass'])

tmdb_api = credentials['tmdb']

# Radarr API

# Lookup movie


def lookup_movie(term, fields):
    response = requests.request(
        "GET", radarr_url + "/api/v3/movie/lookup?term=" + term, headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return "Error: " + response.status_code

    fields = fields.split(',')
    results = []
    for movie in response.json():
        result = {}
        for field in fields:
            result[field] = movie[field]
        results.append(result)

    return json.dumps(results)


def add_movie(tmdbId, title, year):
    payload = {
        'tmdbId': tmdbId,
        'title': title,
        'year': year
    }

    response = requests.request("POST", radarr_url + "/api/v3/movie",
                                headers=radarr_headers, auth=radarr_auth, data=payload)

    if response.status_code != 201:
        return "Error: " + response.status_code

    return "Added " + title + " to your collection."


# Init messages
initMessages = [
    {
        "role": "assistant",
        "content": """You are a media management assistant called CineMatic, you have api access to interface with adding/reading from a media server, querying TMDB, storing memories per user you interact with. You are enthusiastic, knowledgeable and passionate about all things media.

        The assistant never replies with a System call, the software above handles these calls and returns a response to the assistant
        Valid commands are:
        CMDRET, run command and expect a return, e.g. lookup movie, when using this you must await a reply!
        CMD, run command, e.g. add movie
        REPLY, reply to user, e.g. Added movie to collection

        The assistant replies exclusively with commands in []
        """
    },
    {
        "role": "user",
        "content": "Please add the first two harry potter films"
    },
    {
        "role": "assistant",
        "content": "[CMDRET:RADARR:movie_lookup:Harry Potter:title,year,tmdbId]"
    },
    {
        "role": "assistant",
        "content": "[RES:[{'title': 'Harry Potter and the Philosopher's Stone', 'year': 2001, 'tmdbId': 671}, {'title': 'Harry Potter and the Chamber of Secrets', 'year': 2002, 'tmdbId': 672}, {'title': 'Harry Potter and the Prisoner of Azkaban', 'year': 2004, 'tmdbId': 673}]]"
    },
    {
        "role": "assistant",
        "content": "[REPLY:I have found the following movies, Harry Potter and the Philosopher's Stone (2001), Harry Potter and the Chamber of Secrets (2002), would you like these added?]"
    },
    {
        "role": "user",
        "content": "Yes please add both"
    },
    {
        "role": "assistant",
        "content": "[CMD:RADARR:movie_post:{'tmdbId':671,'title':'Harry Potter and the Philosopher's Stone','year':2001}][CMD:RADARR:POST:/api/v3/movie:{'tmdbId':672,'title':'Harry Potter and the Chamber of Secrets','year':2002}][REPLY:Added Harry Potter and the Philosopher's Stone 2001 and Harry Potter and the Chamber of Secrets 2002 to your collection.]"
    },

    {
        "role": "user",
        "content": "Add the first Iron Man movie"
    },
    {
        "role": "assistant",
        "content": "[CMDRET:RADARR:movie_lookup:Iron Man 1:title,year,tmdbId]"
    },
    {
        "role": "assistant",
        "content": "RES:[{'title': 'Iron Man', 'year': 2008, 'tmdbId': 1726}, {'title': 'Tetsuo: The Iron Man', 'year': 1989, 'tmdbId': 41428}]"
    },
    {
        "role": "assistant",
        "content": "[REPLY:I have found the following movie, Iron Man (2008), would you like me to add it to your collection?]"
    },
    {
        "role": "user",
        "content": "Yeah"
    },
    {
        "role": "assistant",
        "content": "[CMD:RADARR:movie_post:{'tmdbId':1726,'title':'Iron Man','year':2008}][REPLY: Added Iron Man 2008 to your collection.]"
    },

    {
        "role": "user",
        "content": "Add watchmen"
    },
    {
        "role": "assistant",
        "content": "Is this a movie or tv series?"
    },
    {
        "role": "user",
        "content": "Movie"
    },
    {
        "role": "assistant",
        "content": "[CMDRET:RADARR:movie_lookup:Watchmen:title,year,tmdbId]"
    },
    {
        "role": "assistant",
        "content": "[RES:[{'title': 'Watchmen', 'year': 2009, 'tmdbId': 13183}, {'title': 'Watchmen', 'year': 2001, 'tmdbId': 343072}, {'title': 'The Night Watchmen', 'year': 2015, 'tmdbId': 331113}, {'title': 'The Night Watchmen', 'year': 2017, 'tmdbId': 398798}, {'title': 'The Watchmen', 'year': 2017, 'tmdbId': 433354}, {'title': 'Tales of the Black Freighter', 'year': 2009, 'tmdbId': 16440}"
    },
    {
        "role": "assistant",
        "content": "[REPLY:I have found multiple results, I assume you mean the 2009 movie, would you like me to add it to your collection? There is also a 2001 movie, a 2015 movie called The Night Watchmen, and a 2017 movie called The Watchmen. If you did mean the 2009 movie, might I suggest also adding the 2009 movie Tales of the Black Freighter which is a short film based on the Watchmen comic book series.]"
    },
    {
        "role": "user",
        "content": "Yes the 2009 movie and the short film sounds good too"
    },
    {
        "role": "assistant",
        "content": "[CMD:RADARR:movie_post:{'tmdbId':13183,'title':'Watchmen','year':2009}][CMD:RADARR:movie_post:{'tmdbId':16440,'title':'Tales of the Black Freighter','year':2009}][REPLY: Added Watchmen and Tales of the Black Freighter from 2009 to your collection.]"
    }
]

currentMessage = initMessages.copy()

# Loop prompting for input
for i in range(10):
    userText = input("User: ")
    currentMessage.append({
        "role": "user",
        "content": userText
    })

    # Run a chat completion
    response = openai.ChatCompletion.create(
        model = "gpt-4", #"gpt-3.5-turbo"
        messages = currentMessage,
        temperature = 0.7
    )
    responseMessage = response['choices'][0]['message']['content']
    print("Assistant: " + responseMessage)
    commands = '][' in responseMessage and responseMessage.split('][') or [responseMessage]

    # Execute commands and return responses
    if 'CMDRET' in responseMessage:
        returnMessage = ''
        for command in commands:
            command = command[1:-1].split(':')
            if command[1] == 'RADARR':
                if command[2] == 'movie_lookup':
                    returnMessage = "[RES:" + lookup_movie(command[3], command[4]) + "]"

        currentMessage.append({
            "role": "assistant",
            "content": returnMessage
        })
        print("System: " + returnMessage)

        # Run a chat completion
        response = openai.ChatCompletion.create(
            model = "gpt-4",
            messages = currentMessage,
            temperature = 0.7
        )
        responseMessage = response['choices'][0]['message']['content']
        print("Assistant: " + responseMessage)