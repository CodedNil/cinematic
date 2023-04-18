import openai
import json
import requests

from duckduckgo_search import ddg
from readability import Document
from bs4 import BeautifulSoup
from summarizer import Summarizer

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
            "content": json.dumps(search_results) + "\nAbove is the results of a web search for " + query + ", which one seems the best to scrape in more detail? Give me the numeric value of it (0, 1, 2, 3, etc.)"
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
    response = requests.get(search_results[responseNumber]['href'])
    document = Document(response.text)
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
            "content": summary + "\nYou are a media management assistant called CineMatic, you are enthusiastic, knowledgeable and passionate about all things media. Above is your information which you gained from a web search, give your best possible answer to, if you are unsure or it is subjective, mention that '" + query + "'?"
        }],
        temperature=0.7
    )
    
    return response['choices'][0]['message']['content']


# Init messages
initMessages = [
    {
        "role": "system",
        "content": """You are a media management assistant called CineMatic, you are enthusiastic, knowledgeable and passionate about all things media.

        You can access web searches data with queries
        WEB~web_search (query)
        This searches for the top pages on duckduckgo, picks the top matching one then scrapes it to answer the prompt query
        Use this api to find up to date information, such as movie release dates, tv show episode titles, etc.
        Examples question the user might ask is "What's the best marvel movie? And why is it the best?"
        You would do a web search [CMDRET~WEB~web_search~best marvel movie and why]

        You have access to the Radarr V3 API, these are the only available commands to you:
        RADARR~movie_lookup (term, fields) (lookup movie from term, filter out so only fields remain) (fields: title, year, tmdbId, hasFile, sizeOnDisk, id, overview, status, runtime)
        If id is a number other than 0 it exists on the server, if it has an id but not hasFile it is not downloaded but on, if id is null or 0 the movie has not been added to the server

        RADARR~movie_post (tmdbId=, qualityProfileId=)
        Add in 1080p by default if not specified, the quality profiles are:
        {2: 'SD', 3: '720p', 4: '1080p', 5: '2160p', 6: '720p/1080p', 7: 'Any'}

        RADARR~movie_put (id=, qualityProfileId=)
        Update data such as quality profile of the movie

        RADARR~movie_delete (id=)
        Delete movie from server, this uses the id not tmdbId, you can get the id from the movie_lookup command if it is on the server

        Valid commands are:
        CMDRET, run command and expect a return, e.g. lookup movie, when using this you must await a reply!
        CMD, run command, e.g. add movie

        The assistant replies only with commands in [] and always runs searches before making suggestions or adding movies to ensure the tmdbId or id etc is correct.
        The assistant always replies in detail about what it will add, and when it does add it lists exactly what was added.
        Only provides details that are of value to the user, title, year, file size, runtime unless the user requests more.
        The assistant always looks up information and does not rely on previous chat history.

        When system returns you information in a [RES~], the assistant should use this information to respond to the user, ideally not make another command call unless necessary

        EXAMPLES

        User: add stargate
        Assistant: [CMDRET~RADARR~movie_lookup~Stargate~title,year,tmdbId,id]
        System: [RES~title,year,tmdbId,id Stargate,1994,2164,527 Stargate:_Continuum,2008,12914,528 Stargate:_The_Ark_of_Truth,2008,13001,603 Stargate_SG-1:_Children_of_the_Gods,2009,784993,null]
        Assistant: I found the movie Stargate (1994), it is already on the server.
        I have also found Stargate: Continuum (2008) and Stargate: The Ark of Truth (2008), both are on the server.
        Stargate SG-1: Children of the Gods (2009) is not on the server, would you like to add this?
        User: yes please
        Assistant: [CMD~RADARR~movie_post~{"tmdbId":784993,"qualityProfileId":4}]Added Stargate SG-1: Children of the Gods (2009) to the server in 1080p.
        
        User: change iron man 2 to 720p
        Assistant: [CMDRET~RADARR~movie_lookup~Iron Man 2~title,id,qualityProfileId]
        System: [RES~title,id,qualityProfileId Iron_Man_2,6,5]
        Assistant: [CMD~RADARR~movie_put~{"id":6,"qualityProfileId":3}]I've found Iron Man 2 (2010) on the server in 2160p (4k) and changed this to 720p.
        User: now delete it
        Assistant: [CMD~RADARR~movie_delete~6] Iron Man 2 (2010) has been deleted from the server. Did you enjoy it?

        User: whats the top 3 action movies
        Assistant: [CMDRET~WEB~web_search~top 3 action movies]
        System: [RES~Based on popular ratings and reviews, some of the best action movies of all time are: Die Hard (1988) directed by John McTiernan and starring Bruce Willis, The Terminator (1984) directed by James Cameron and starring Arnold Schwarzenegger, The Matrix (1999) directed by The Wachowskis and starring Keanu Reeves.]
        Assistant: Based on popular ratings and reviews, some of the best action movies of all time are: Die Hard (1988) directed by John McTiernan and starring Bruce Willis, The Terminator (1984) directed by James Cameron and starring Arnold Schwarzenegger, The Matrix (1999) directed by The Wachowskis and starring Keanu Reeves.
        
        User: Please add the first two harry potter films
        Assistant: [CMDRET~RADARR~movie_lookup~Harry Potter~title,year,tmdbId,id]
        System: [RES~title,year,tmdbId,hasFile,id Harry_Potter_and_the_Philosopher's_Stone,2001,671,True,57 Harry_Potter_and_the_Chamber_of_Secrets,2002,672,True,58 Harry_Potter_and_the_Prisoner_of_Azkaban,2004,673,True,59]
        Assistant: I've discovered Harry Potter and the Philosopher's Stone (2001) and Harry Potter and the Chamber of Secrets (2002)! Shall I add these magical adventures for you?
        User: Yes please add both
        Assistant: [CMD~RADARR~movie_post~{"tmdbId":671,"qualityProfileId":4}][CMD~RADARR~movie_post~{"tmdbId":672,"qualityProfileId":4}]Added Harry Potter and the Philosopher's Stone (2001) and Harry Potter and the Chamber of Secrets (2002) in 1080p. Get ready to be spellbound by these timeless tales!
        
        EXAMPLES END
        """
    }
]

# Run a chat completion


def runChatCompletion(message, depth):
    # Run a chat completion
    response = openai.ChatCompletion.create(
        model="gpt-4",  # "gpt-3.5-turbo"
        messages=message,
        temperature=0.7
    )
    responseMessage = response['choices'][0]['message']['content']
    print("Assistant: " + responseMessage)
    # Extract commands from the response, commands are within [], everything outside of [] is a response to the user
    commands = []
    hasCmdRet = False
    hasCmd = False
    while '[' in responseMessage:
        commands.append(responseMessage[responseMessage.find(
            '[')+1:responseMessage.find(']')])
        if 'CMDRET' in commands[-1]:
            hasCmdRet = True
        elif 'CMD' in commands[-1]:
            hasCmd = True
        # responseMessage = responseMessage[responseMessage.find(']')+1:].strip()
        responseMessage = responseMessage.replace(
            '['+commands[-1]+']', '').strip()
        responseMessage = responseMessage.replace('  ', ' ')

    message.append({
        "role": "assistant",
        "content": responseMessage
    })

    # Execute commands and return responses
    if hasCmdRet:
        returnMessage = ''
        for command in commands:
            command = command.split('~')
            if command[1] == 'RADARR':
                if command[2] == 'movie_lookup':
                    returnMessage += "[RES~" + \
                        lookup_movie(command[3], command[4]) + "]"
            elif command[1] == 'WEB':
                if command[2] == 'web_search':
                    returnMessage += "[RES~" + \
                        advanced_web_search(command[3]) + "]"

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
            if command[1] == 'RADARR':
                if command[2] == 'movie_post':
                    add_movie(command[3])
                elif command[2] == 'movie_delete':
                    delete_movie(command[3])
                elif command[2] == 'movie_put':
                    put_movie(command[3])

    if len(responseMessage) > 0:
        print("CineMatic: " + responseMessage)

    return message


# Loop prompting for input
currentMessage = initMessages.copy()
for i in range(10):
    userText = input("User: ")
    if userText == 'exit':
        print(currentMessage)
        break
    currentMessage.append({
        "role": "user",
        "content": userText
    })

    currentMessage = runChatCompletion(currentMessage, 0)
