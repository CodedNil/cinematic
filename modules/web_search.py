import json
import openai
import requests
from modules.module_logs import ModuleLogs

from duckduckgo_search import ddg
from readability import Document
from bs4 import BeautifulSoup
from summarizer import Summarizer

# BERT
import logging
from transformers import logging as transformers_logging

# Suppress BERT logging
logging.basicConfig(level=logging.INFO)
transformers_logging.set_verbosity(transformers_logging.ERROR)


class WebAPI:
    """Class to handle web search and scraping"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key

        # Create logs
        self.logs = ModuleLogs("web")

    def basic(self, query: str = "", numResults: int = 4) -> dict:
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

    def advanced(self, query: str = "") -> str:
        """Perform a DuckDuckGo Search, parse the results through gpt to get the top pick site based on the query, then scrape that website through gpt to return the answer to the prompt"""
        search_results = self.basic(query, 8)

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
        # Log the response
        self.logs.log("website_choice", search_results, query, response)

        responseNumber = response["choices"][0]["message"]["content"]
        # Test if the response number str is single digit number
        if (
            not responseNumber.isdigit()
            or int(responseNumber) > len(search_results) - 1
        ):
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
                    "content": "You are a media management assistant called CineMatic, you are enthusiastic, knowledgeable and passionate about all things media. If you are unsure or it is subjective, mention that",
                },
                {"role": "system", "content": summary},
                {
                    "role": "user",
                    "content": f"Above is the results of a web search from {search_results[responseNumber]['href']} that was just performed to gain the latest information, give your best possible answer to '{query}'?",
                },
            ],
            temperature=0.7,
        )
        # Log the response
        self.logs.log(
            "answer", summary.replace("\n", " - "), query, response.replace("\n", " - ")
        )

        return response["choices"][0]["message"]["content"]
