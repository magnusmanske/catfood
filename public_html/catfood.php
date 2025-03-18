<?php

error_reporting(E_ERROR|E_CORE_ERROR|E_ALL|E_COMPILE_ERROR);
ini_set('display_errors', 'On');

set_time_limit ( 5*60 ) ; # 5min
include_once ( 'php/wikiquery.php' ) ;
require_once ( 'php/ToolforgeCommon.php' ) ;
$tfc = new ToolforgeCommon('catfood') ;

function get_image_url ( $lang , $image , $project = "wikipedia" ) {
  global $tfc ;
  $wiki = $tfc->getWikiForLanguageProject ( $lang , $project ) ;
  return "//".$tfc->getWebserverForWiki($wiki)."/wiki/Special:Redirect/file/".$tfc->urlEncode($image);
}

function get_thumbnail_url ( $lang , $image , $width , $project = "wikipedia" ) {
  global $tfc ;
  $wiki = $tfc->getWikiForLanguageProject ( $lang , $project ) ;
  return "//".$tfc->getWebserverForWiki($wiki)."/wiki/Special:Redirect/file/".$tfc->urlEncode($image)."?width={$width}";
}

$test = isset ( $_REQUEST['test'] ) ;

$language = $tfc->getRequest ( 'language' , 'commons' ) ;
$project = $tfc->getRequest ( 'project' , 'wikimedia' ) ;
$depth = floor ( $tfc->getRequest ( 'depth' , 0 ) ) ;
$namespace = floor ( $tfc->getRequest ( 'namespace' , 6 ) ) ;

$tsf = "D, d M Y H:i:s " ;
$tstz = "GMT" ;

function escape4xml ( $text ) {
	$doc = new DOMDocument();
	$fragment = $doc->createDocumentFragment();
	$fragment->appendChild($doc->createTextNode($text));
	return $doc->saveXML($fragment);
}

function print_before_items () {
  global $motd , $category , $images , $firstcat , $user , $size , $last , $test , $project , $language , $depth ;
  global $tsf , $tstz ;

  $ts = $images[$firstcat]->thets ; #img_timestamp ;
  if ( $ts == '' ) $ts = date ( $tsf ) . $tstz ; // Dirty hack; using current date
  else {
  	$tso = $ts ;
	$ts = DateTime::createFromFormat ( 'Y-m-d H:i:s' , $tso ) ;
	if ( $ts === false ) $ts = new DateTime ( $tso ) ;
	$ts = $ts->format ( $tsf ) . $tstz ;
  }

  $d = '' ;
  if ( $depth > 0 ) $d = "(depth $depth) " ;

  $ownpath = '//'.$_SERVER["SERVER_NAME"].escape4xml($_SERVER['REQUEST_URI']) ;
  print '<?xml version="1.0" encoding="UTF-8"?>
  <rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
  <atom:link href="'.$ownpath.'" rel="self" type="application/rss+xml" />' ;

  $info = $category ;
  if ( $info == "" && $user != "" ) $info = "Uploads by User:" . $user ;
  if ( $info == "" && $motd != "" ) $info = "Media file of the Day" ;

  print "<title>$info $d- $language.$project.org</title>
  <link>http://tools.wmflabs.org/catfood/catfood.php</link>" ;

  if ( $category != "" )
    print "<description>Category feed for category \"$category\" from $language.$project.org</description>" ;
  else if ( $user != "" )
    print "<description>Feed for user \"$user\" from $language.$project.org</description>" ;
  else if ( $motd != "" )
    print "<description>Feed for Media file of the Day from $language.$project.org</description>" ;

  print "<language>en-us</language><copyright>Feed: GNU Free Documentation License; Images: see description page</copyright>" ;
  print "<pubDate>{$ts}</pubDate><lastBuildDate>{$ts}</lastBuildDate>" ;

  print "<generator>CatFood</generator>
  <docs>http://tools.wmflabs.org/catfood/catfood.php</docs>

  <image>
    <url>" ;

    if ( $language == 'commons' ) print "http://upload.wikimedia.org/wikipedia/commons/7/79/Wiki-commons.png" ;
    else print get_thumbnail_url ( "commons" , "Wikipedia-logo-$language.png" , 135 , "wikimedia" ) ;

    print "</url>
    <title>$info $d- $language.$project.org</title>
    <link>http://tools.wmflabs.org/catfood/catfood.php</link>
  </image>
  " ;
}

function get_thumb_url ( &$c , $size ) {
  global $language , $project ;
  if ( $c->img_width < 1 ) { # Do nothing
    if ( $c->img_media_type == "AUDIO" ) {
      if ( $size < 200 ) $s = $size ;
      else $s = 200 ;
      return "http://upload.wikimedia.org/wikipedia/commons/thumb/4/47/Sound-icon.svg/{$s}px-Sound-icon.svg.png" ;
    }
  } else if ( $c->img_width < $size && $c->img_height < $size ) { # Image smaller than max px, return direct URL (no thumbnail)
    return get_image_url ( $language , $c->img_name , $project ) ;
  } else if ( $c->img_width < $c->img_height ) {
    $size = $c->img_width * $size / $c->img_height ;
  }
  $image = $c->img_name ;

  $wq = new WikiQuery ( $language , $project ) ;
  $ret = $wq->get_image_data ( "File:$image" , round ( $size ) ) ;
  $ret = $ret['imageinfo'][0]['thumburl'] ;
//  $ret = get_thumbnail_url ( $language , $image , round ( $size ) , $project ) ;
  return $ret ;
}

function get_licenses ( $arr ) {
  $ret = [] ;
  foreach ( $arr AS $c ) {
    $c = ucfirst ( $c ) ;
    if ( substr ( $c , 0 , 2 ) == "PD" ) $c = "Public domain" ;
    else if ( stripos ( $c , "GFDL" ) !== false ) $c = "GFDL" ;
    else if ( stripos ( $c , "CC-BY-SA" ) !== false ) $c = "CC-BY-SA" ;
    else if ( stripos ( $c , "CC-BY" ) !== false ) $c = "CC-BY" ;
    else if ( stripos ( $c , "CC-SA" ) !== false ) $c = "CC-SA" ;
    else continue ;

    $c2 = str_replace ( " " , "_" , $c ) ;
    $ret[$c2] = "<a href=\"http://en.wikipedia.org/wiki/$c2\">$c</a>" ;
  }

  $ret = implode ( "," , $ret ) ;
  if ( $ret != "" ) $ret = "Licensing : " . $ret ;
  return $ret ;
}

function expand_ts ( $timestamp ) {
  return substr($timestamp,0,4)."-".substr($timestamp,4,2)."-".substr($timestamp,6,2)." ".substr($timestamp,8,2).":".substr($timestamp,10,2) ;
}


$db = $tfc->openDB ( $language , "wikipedia" ) ;

$motd = $tfc->getRequest ( "motd" , "" ) ;
$category = trim ( str_replace ( "_" , " " , $tfc->getRequest ( "category" , "" ) ) ) ;
$user = trim ( str_replace ( "_" , " " , $tfc->getRequest ( "user" , "" ) ) ) ;
$size = $tfc->getRequest ( "size" , "300" ) ;
$last = $tfc->getRequest ( "last" , ( $motd == "" ) ? "10" : "5" ) ;
if ( $last < 1 ) $last = 1 ;
if ( $last > 100 ) $last = 100 ;

if ( $motd . $user . $category == "" || false === $db ) {
  $mytitle = ucfirst ( $project ) . " " . ucfirst ( $language ) ;
  header('Content-type: text/html; charset=utf-8');
  print '<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">' . "\n\n" ;
  print $tfc->getCommonHeader ( "CatFood (Category news feeder)" ) ;
  print "This is a category-based RSS feed for <a href='http://".htmlspecialchars($language).".".htmlspecialchars($project).".org'>".htmlspecialchars($mytitle)."</a>.<br/>
The images are ordered based on the time of the addition of the image to the category, latest additions first.<br/>
Alternatively, you can see the last images uploaded by a user.<br/>
You need to supply at least a category or a user name to get a feed.<br/>
Enter the information below, or use URL parameters (click on \"Do it!\" for a demo).
<form method='get'>
<table class='table table-condensed'>
<tr><th>Language</th><td><input type='text' name='language' value='".htmlspecialchars($language)."' /></td><td>e.g., \"en\" or \"commons\"</td></tr>
<tr><th>Project</th><td><input type='text' name='project' value='".htmlspecialchars($project)."' /></td><td>e.g. \"wikipedia\"</td></tr>
<tr><th>Category</th><td><input type='text' name='category' value='Featured pictures on Wikimedia Commons' class='span4' /></td><td>without the \"Category:\" prefix</td></tr>
<tr><th>Depth</th><td><input type='number' name='depth' value='".htmlspecialchars($depth)."' /></td><td>0=just this category</td></tr>
<tr><th>Namespace</th><td><input type='number' name='namespace' value='".htmlspecialchars($namespace)."' /></td><td>6=images; 0=article</td></tr>
<tr><th>User</th><td><input type='text' name='user' value='' /></td><td>".htmlspecialchars($mytitle)." user name, <i>instead</i> of category</td></tr>
<tr><th>Size</th><td>
<label class='radio inline'><input type='radio' name='size' value='200' id='size200' />200px</label>
<label class='radio inline'><input type='radio' name='size' value='250' id='size250' />250px</label>
<label class='radio inline'><input type='radio' name='size' value='300' id='size300' checked />300px</label>
</td><td>maximum height/width
</td></tr>
<tr><th>Number</th><td><input type='text' name='last' value='10' /></td><td>last X images/pages; 1-100</td></tr>
<tr><td/><td><input type='submit' name='doit' value='Do it' class='btn btn-primary' /></td><td></td></tr>
</table>
</form>
Or get a feed of the <a href=\"http://catfood.toolforge.org/catfood.php?motd=1\">Media file of the Day</a>!
</div></div></body></html>" ;
  exit ;
}




$cat2 = $db->real_escape_string ( $category ) ;
$cat2 = str_replace ( " " , "_" , $cat2 ) ;

$user2 = $db->real_escape_string ( $user ) ;
$user2 = str_replace ( "_" , " " , $user2 ) ;

$firstcat = "" ;
$images = [] ;
$pages = [] ;

if ( $tfc->use_file_table ) {
	$image_table = 'file,filerevision,filetypes,actor';

	$image_query = 'filerevision.fr_width AS img_width,
		filerevision.fr_height AS img_height,
		filetypes.ft_media_type AS img_media_type,
		actor.actor_name AS img_user_text,
		filerevision.fr_timestamp AS img_timestamp,
		file.file_name AS img_name,
		filerevision.fr_size AS img_size';

	$image_where = ' AND file.file_latest=filerevision.fr_id AND file.file_type=filetypes.ft_id AND filerevision.fr_actor=actor.actor_id';
	$img_name = 'file.file_name';
} else {
	$image_table = 'image_compat';
	$image_query = 'image_compat.*';
	$image_where = '';
	$img_name = 'img_name';
}


if ( $motd != "" ) { // Medium of the Day

} else if ( $namespace != 6 ) {

  	if ( $depth == 0 ) $cats = '"'.$cat2.'"' ;
  	else {
  		$cats = [] ;
  		$tfc->findSubcats ( $db , [ $cat2 ]  , $cats , $depth ) ; // FIXME!!!
//		$cats = db_get_articles_in_category ( $language , $cat2 , $depth-1 , 14 ) ;
		$cats = '"' . implode ( '","' , $cats ) . '"' ;
	}

    if ( $tfc->use_new_categorylinks ) {
      $sql = "SELECT page.*,categorylinks.*,revision_compat.*,lt_title AS cl_to FROM page,categorylinks,linktarget,revision_compat WHERE cl_target_id=lt_id AND lt_namespace=14 AND page_id=cl_from AND cl_to IN ( $cats ) AND rev_id=page_latest AND rev_page=page_id AND page_namespace=$namespace ORDER BY rev_timestamp DESC LIMIT $last" ;
    } else {
      $sql = "SELECT * FROM page,categorylinks,revision_compat WHERE page_id=cl_from AND cl_to IN ( $cats ) AND rev_id=page_latest AND rev_page=page_id AND page_namespace=$namespace ORDER BY rev_timestamp DESC LIMIT $last" ;

    }
    $result = $tfc->getSQL($db,$sql);
    while($o = $result->fetch_object()){
		$thets = expand_ts ( $o->rev_timestamp ) ;

		$o->thets = $thets ;
		$o->img_name = $o->page_title ;
		$o->img_timestamp = $o->rev_timestamp ;
		$o->img_user_text = $o->rev_user_text ;
		$o->img_size = $o->rev_len ;
		$images[$thets.$o->page_title] = $o ;
		$pages[] = $o->page_id ;
		if ( $firstcat == "" ) {
			$firstcat = $thets ;
		}
	}

} else {
	if ( $category != "" ) { // Get last X images that were uploaded in that category
		if ( $depth == 0 ) $cats = '"'.$cat2.'"' ;
		else {
			$tfc->findSubcats ( $db , array ( $cat2 )  , $cats , $depth ) ;
			$cats = '"' . implode ( '","' , $cats ) . '"' ;
		}


    if ( $tfc->use_new_categorylinks ) {
      $sql = "SELECT page.*,{$image_query},categorylinks.*,lt_title AS cl_to
      	FROM page,{$image_table},categorylinks,linktarget
		WHERE cl_target_id=lt_id
		AND lt_namespace=14 AND
		page_id=cl_from AND
		{$img_name}=page_title
		AND cl_to IN ($cats )
		{$image_where}
		ORDER BY cl_timestamp DESC
		LIMIT {$last}" ;
    } else {
      $sql = "SELECT page.*,categorylinks.*,{$image_query}
      	FROM page,categorylinks,{$image_table}
		WHERE page_id=cl_from
		AND {$img_name}=page_title
		AND cl_to IN ($cats )
		{$image_where}
		ORDER BY cl_timestamp DESC
		LIMIT {$last}" ;
    }

	} else if ( $user != "" ) { // Get last X images that were uploaded by that user
		$sql = "SELECT {$image_query} FROM page,{$image_table}
			WHERE img_user_text=\"{$user2}\"
			AND img_name=page_title
			AND page_namespace={$namespace}
			{$image_where}
			ORDER BY img_timestamp
			DESC LIMIT {$last}" ;
	}

  $result = $tfc->getSQL($db,$sql);
	while($o = $result->fetch_object()){
		$thets = expand_ts ( $o->img_timestamp ) ;
		if ( $category != '' and $o->cl_timestamp > $thets ) $thets =  $o->cl_timestamp ;
		$o->thets = $thets ;
		$images[$thets] = $o ;
		$pages[] = $o->page_id ;
		if ( $firstcat == "" ) $firstcat = $thets ;
	}
}

if ( count ( $images ) == 0 ) {
  header('Content-type: text/html; charset=utf-8');
  print '<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">' . "\n\n" ;
//  print get_common_header ( "catfood.php" , "Catfood (Category news feeder)" ) ;
  print "No files found." ;
  exit ;
}

header('Content-type: application/rss+xml; charset=utf-8');

// Get licensing information
$cats = [] ;
if ( $tfc->use_new_categorylinks ) {
  $sql = "SELECT categorylinks.*,lt_title AS cl_to FROM categorylinks,linktarget WHERE cl_target_id=lt_id AND lt_namespace=14 AND cl_from IN (" . implode ( "," , $pages ) . ")" ;
} else {
  $sql = "SELECT * FROM categorylinks WHERE cl_from IN (" . implode ( "," , $pages ) . ")" ;
}
$result = $tfc->getSQL($db,$sql);
while($o = $result->fetch_object()){
  if ( !isset ( $cats[$o->cl_from] ) ) $cats[$o->cl_from] = [] ;
  $cats[$o->cl_from][] = $o->cl_to ;
}


// Output
print_before_items () ;

$cnt = 0 ;
foreach ( $images AS $c ) {
  $title = str_replace ( "_" , " " , $c->img_name ) ;

  if ( isset ( $c->img_width ) and $c->img_width < $size ) $thumburl = get_image_url ( $language , $c->img_name , 'wikipedia' ) ;
  else $thumburl = get_thumb_url ( $c , $size ) ;

  $timestamp = expand_ts ( $c->img_timestamp ) ;
  $ts = strtotime ( $timestamp ) ;
  $timestamp = date ( $tsf , $ts ) . $tstz ;
  $ts = strtotime ( $c->thets ) ;
  $realts = date ( $tsf , $ts ) . $tstz ;

  $nicesize = number_format ( $c->img_size , 0 , "" , "." ) ;

  $descurl = "http://$language.$project.org/wiki/" ;
  if ( $namespace == 6 ) $descurl .= "File:" ;
  $descurl .= urlencode ( $c->img_name ) ;
  $guid = $descurl ;
  if ( isset ( $cats[$c->page_id] ) ) $licenses = get_licenses ( $cats[$c->page_id] ) ;
  else $licenses = "" ;
  if ( $licenses == "" ) $licenses = "For license information, see the image description page <a href=\"{$descurl}\">here</a>." ;

  $desc = "" ;
//  if ( $test ) $desc = count ( $images ) . " : " . $cnt++ ;
  if ( $namespace == 6 ) {
  	$desc .= "<a href=\"$descurl\"><img border=\"0\" src=\"$thumburl\" /></a><br/>\n" ;
  	if ( isset ( $c->desc ) ) $desc .= $c->desc . "<br/>" ;
  }

  if ( $namespace == 6 ) $desc .= "Uploaded" ;
  else $desc .= "Edited" ;

  $desc .= " by user \"<a href=\"http://$language.$project.org/wiki/User:{$c->img_user_text}\">{$c->img_user_text}</a>\" on {$timestamp}<br/>\n" ;

  if ( $namespace != 6 ) {
  	$desc .= "\"<i>" . htmlspecialchars ( $c->rev_comment ) . "</i>\"<br/>" ;
  }

  if ( isset ( $c->cl_timestamp ) && $namespace == 6 ) {
  	$desc .= "Added to category on {$realts}<br/>\n" ;
  	$guid .= " - " . $realts ;
  }

  if ( $namespace != 6 ) {
  	$desc .= "New text length: " ;
  } else if ( $c->img_media_type == "UNKNOWN" ) {
    $desc .= "Original file: " ;
  } else if ( $c->img_media_type == "BITMAP" || $c->img_media_type == "DRAWING" ) {
    $desc .= "Original image: {$c->img_width}&times;{$c->img_height} pixel; " ;
  } else {
    $desc .= "Original " . strtolower ( $c->img_media_type ) . " file: " ;
  }
  $desc .= "$nicesize bytes.<br/>\n" ;
  if ( $namespace == 6 ) $desc .= $licenses ;

  $desc = str_replace ( "&" , "&amp;" , $desc ) ;
  $desc = str_replace ( "<" , "&lt;" , $desc ) ;
  $desc = str_replace ( ">" , "&gt;" , $desc ) ;

  print "\n<item>\n" ;
  print " <title>" . escape4xml($title) . "</title>\n" ;
  print " <pubDate>$realts</pubDate>\n" ;
  print " <link>" . $descurl . "</link>\n" ;
  print " <guid isPermaLink=\"false\">$guid</guid>\n" ;
  print " <description>\n" ;
  print "$desc\n" ;
  print " </description>\n" ;
  print "</item>\n\n" ;
}

print '</channel></rss>' ;

$tfc->logToolUse('','rss') ;


?>
